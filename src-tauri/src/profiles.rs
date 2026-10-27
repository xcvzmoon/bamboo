use std::{
    fs::{self, DirBuilder},
    io::Write,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Mutex,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, Generate, Key, KeyInit, Payload},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::{NamedTempFile, TempDir};
use uuid::Uuid;
use zeroize::Zeroizing;

#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("PROFILE_IO: Could not access profile files")]
    Io(#[from] std::io::Error),
    #[error("PROFILE_INPUT: Enter a name, an HTTP(S) login URL, and an executable browser path")]
    Input,
    #[error("PROFILE_BUSY: Finish or cancel the current login setup first")]
    Busy,
    #[error("PROFILE_NOT_FOUND: Profile or login setup no longer exists")]
    NotFound,
    #[error(
        "PROFILE_KEYCHAIN: Cannot access the encryption key. Allow Bamboo access in macOS Keychain and try again"
    )]
    Keychain,
    #[error("PROFILE_ENCRYPTION: Saved profile could not be encrypted or decrypted")]
    Encryption,
    #[error(
        "PROFILE_BROWSER: Login browser failed or closed. Check its executable path and try again"
    )]
    Browser,
    #[error("PROFILE_STATE: Browser returned invalid or oversized session state")]
    State,
}

impl ProfileError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "PROFILE_IO",
            Self::Input => "PROFILE_INPUT",
            Self::Busy => "PROFILE_BUSY",
            Self::NotFound => "PROFILE_NOT_FOUND",
            Self::Keychain => "PROFILE_KEYCHAIN",
            Self::Encryption => "PROFILE_ENCRYPTION",
            Self::Browser => "PROFILE_BROWSER",
            Self::State => "PROFILE_STATE",
        }
    }
}

type Result<T> = std::result::Result<T, ProfileError>;

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct SessionProfile {
    pub id: Uuid,
    pub name: String,
    pub login_url: String,
    pub browser: PathBuf,
    pub captured_at: u64,
    #[serde(skip_deserializing)]
    pub needs_refresh: bool,
}

#[derive(Serialize, Deserialize)]
struct SavedProfile {
    profile: SessionProfile,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}

pub struct ProfileSnapshot {
    pub id: Uuid,
    pub revision: String,
    pub state: Zeroizing<Vec<u8>>,
}

#[derive(Serialize, Deserialize)]
struct PendingProfile {
    run_id: Uuid,
    profile_id: Uuid,
    revision: String,
    update: ProfileUpdate,
}

#[derive(Serialize, Deserialize)]
enum ProfileUpdate {
    Session(SavedProfile),
    AuthenticationRequired,
}

#[derive(Clone, Serialize)]
pub struct CaptureStatus {
    pub id: Uuid,
    pub name: String,
    pub refreshing: bool,
}

struct Capture {
    status: CaptureStatus,
    profile: SessionProfile,
    child: Child,
    directory: TempDir,
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.child.stdin.take();
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.child.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

trait ProfileKey: Send + Sync {
    fn load(&self, create: bool) -> Result<Zeroizing<Vec<u8>>>;
}

struct KeychainKey {
    service: String,
}

impl ProfileKey for KeychainKey {
    fn load(&self, create: bool) -> Result<Zeroizing<Vec<u8>>> {
        let entry = keyring::Entry::new(&self.service, "encryption-key")
            .map_err(|_| ProfileError::Keychain)?;
        match entry.get_secret() {
            Ok(secret) if secret.len() == 32 => Ok(Zeroizing::new(secret)),
            Err(keyring::Error::NoEntry) if create => {
                let key = Key::<Aes256Gcm>::generate();
                entry.set_secret(&key).map_err(|_| ProfileError::Keychain)?;
                Ok(Zeroizing::new(key.to_vec()))
            }
            _ => Err(ProfileError::Keychain),
        }
    }
}

pub struct Profiles {
    root: PathBuf,
    resources: PathBuf,
    key: Box<dyn ProfileKey>,
    capture: Mutex<Option<Capture>>,
}

impl Profiles {
    pub fn open(root: PathBuf, resources: PathBuf) -> Result<Self> {
        private_directory(&root)?;
        let temporary = root.join("capture");
        if temporary.exists() {
            fs::remove_dir_all(&temporary)?;
        }
        private_directory(&temporary)?;
        private_directory(&root.join("saved"))?;
        private_directory(&root.join("pending"))?;
        private_directory(&root.join("refresh"))?;
        Ok(Self {
            root,
            resources,
            key: Box::new(KeychainKey {
                service: "com.bamboo.dev.profiles".into(),
            }),
            capture: Mutex::new(None),
        })
    }

    pub fn list(&self) -> Result<Vec<SessionProfile>> {
        let mut profiles = Vec::new();
        for entry in fs::read_dir(self.root.join("saved"))? {
            let entry = entry?;
            if entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                let bytes = fs::read(entry.path())?;
                let saved: SavedProfile =
                    serde_json::from_slice(&bytes).map_err(|_| ProfileError::State)?;
                let mut profile = saved.profile;
                profile.needs_refresh =
                    fs::read_to_string(self.root.join("refresh").join(profile.id.to_string()))
                        .is_ok_and(|revision| revision == hex::encode(Sha256::digest(&bytes)));
                profiles.push(profile);
            }
        }
        profiles.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(profiles)
    }

    pub fn status(&self) -> Result<Option<CaptureStatus>> {
        let mut active = self.capture.lock().map_err(|_| ProfileError::Busy)?;
        if let Some(capture) = active.as_mut()
            && capture.child.try_wait()?.is_some()
        {
            active.take();
            return Err(ProfileError::Browser);
        }
        Ok(active.as_ref().map(|capture| capture.status.clone()))
    }

    pub fn begin(
        &self,
        profile_id: Option<Uuid>,
        name: String,
        login_url: String,
        browser: PathBuf,
        node: &Path,
    ) -> Result<CaptureStatus> {
        let mut active = self.capture.lock().map_err(|_| ProfileError::Busy)?;
        if active.is_some() {
            return Err(ProfileError::Busy);
        }
        let url = reqwest::Url::parse(&login_url).map_err(|_| ProfileError::Input)?;
        if name.trim().is_empty()
            || name.len() > 120
            || !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || !browser.is_absolute()
        {
            return Err(ProfileError::Input);
        }
        if !fs::metadata(&browser)
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        {
            return Err(ProfileError::Browser);
        }
        let profiles = self.list()?;
        if let Some(id) = profile_id
            && !profiles.iter().any(|profile| profile.id == id)
        {
            return Err(ProfileError::NotFound);
        }
        let _key = self.key.load(profiles.is_empty())?;
        let directory = tempfile::tempdir_in(self.root.join("capture"))?;
        let child = Command::new(node)
            .arg(self.resources.join("capture.ts"))
            .arg(&browser)
            .arg(directory.path())
            .arg(&login_url)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| ProfileError::Browser)?;
        let status = CaptureStatus {
            id: Uuid::now_v7(),
            name: name.trim().into(),
            refreshing: profile_id.is_some(),
        };
        let mut capture = Capture {
            profile: SessionProfile {
                id: profile_id.unwrap_or_else(Uuid::now_v7),
                name: status.name.clone(),
                login_url,
                browser,
                captured_at: 0,
                needs_refresh: false,
            },
            status: status.clone(),
            child,
            directory,
        };
        let deadline = Instant::now() + Duration::from_secs(30);
        while !capture.directory.path().join("ready").exists() {
            if capture.child.try_wait()?.is_some() || Instant::now() >= deadline {
                return Err(ProfileError::Browser);
            }
            thread::sleep(Duration::from_millis(50));
        }
        *active = Some(capture);
        Ok(status)
    }

    pub fn cancel(&self, capture_id: Option<Uuid>) -> Result<()> {
        let mut active = self.capture.lock().map_err(|_| ProfileError::Busy)?;
        if capture_id.is_some_and(|id| {
            active
                .as_ref()
                .is_none_or(|capture| capture.status.id != id)
        }) {
            return Err(ProfileError::NotFound);
        }
        active.take();
        Ok(())
    }

    pub fn finish(&self, capture_id: Uuid) -> Result<SessionProfile> {
        let mut active = self.capture.lock().map_err(|_| ProfileError::Busy)?;
        if active
            .as_ref()
            .is_none_or(|capture| capture.status.id != capture_id)
        {
            return Err(ProfileError::NotFound);
        }
        let mut capture = active.take().ok_or(ProfileError::NotFound)?;
        capture
            .child
            .stdin
            .as_mut()
            .ok_or(ProfileError::Browser)?
            .write_all(b"capture\n")?;
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = capture.child.try_wait()? {
                if !status.success() {
                    return Err(ProfileError::Browser);
                }
                break;
            }
            if Instant::now() >= deadline {
                return Err(ProfileError::Browser);
            }
            thread::sleep(Duration::from_millis(50));
        }
        let path = capture.directory.path().join("state.json");
        if fs::metadata(&path)?.len() > 16 * 1024 * 1024 {
            return Err(ProfileError::State);
        }
        let state = Zeroizing::new(fs::read(path)?);
        self.save(capture.profile.clone(), &state)
    }

    fn seal(&self, mut profile: SessionProfile, state: &[u8]) -> Result<SavedProfile> {
        let parsed: serde_json::Value =
            serde_json::from_slice(state).map_err(|_| ProfileError::State)?;
        if !parsed
            .get("cookies")
            .is_some_and(serde_json::Value::is_array)
            || !parsed
                .get("origins")
                .is_some_and(serde_json::Value::is_array)
        {
            return Err(ProfileError::State);
        }
        let key = self.key.load(self.list()?.is_empty())?;
        let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| ProfileError::Encryption)?;
        let nonce = Nonce::generate();
        let ciphertext = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: state,
                    aad: profile.id.as_bytes(),
                },
            )
            .map_err(|_| ProfileError::Encryption)?;
        profile.captured_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ProfileError::State)?
            .as_secs();
        let saved = SavedProfile {
            profile: profile.clone(),
            nonce: nonce.to_vec(),
            ciphertext,
        };
        Ok(saved)
    }

    fn save(&self, profile: SessionProfile, state: &[u8]) -> Result<SessionProfile> {
        let saved = self.seal(profile, state)?;
        self.write_saved(&saved)?;
        Ok(saved.profile)
    }

    fn write_saved(&self, saved: &SavedProfile) -> Result<()> {
        let mut file = NamedTempFile::new_in(self.root.join("saved"))?;
        file.write_all(&serde_json::to_vec(saved).map_err(|_| ProfileError::State)?)?;
        file.as_file().sync_all()?;
        file.persist(self.saved_path(saved.profile.id))
            .map_err(|error| ProfileError::Io(error.error))?;
        fs::File::open(self.root.join("saved"))?.sync_all()?;
        Ok(())
    }

    fn saved_path(&self, id: Uuid) -> PathBuf {
        self.root.join("saved").join(format!("{id}.json"))
    }

    pub fn snapshot(&self, id: Uuid) -> Result<ProfileSnapshot> {
        let _active = self.capture.lock().map_err(|_| ProfileError::Busy)?;
        let state = self.restore_state(id)?;
        let revision = hex::encode(Sha256::digest(fs::read(self.saved_path(id))?));
        Ok(ProfileSnapshot {
            id,
            revision,
            state,
        })
    }

    pub fn stage(
        &self,
        run_id: Uuid,
        snapshot: &ProfileSnapshot,
        candidate: Option<&[u8]>,
    ) -> Result<()> {
        let _active = self.capture.lock().map_err(|_| ProfileError::Busy)?;
        let current = fs::read(self.saved_path(snapshot.id))?;
        if hex::encode(Sha256::digest(&current)) != snapshot.revision {
            return Ok(());
        }
        let update = if let Some(state) = candidate {
            let saved: SavedProfile =
                serde_json::from_slice(&current).map_err(|_| ProfileError::State)?;
            ProfileUpdate::Session(self.seal(saved.profile, state)?)
        } else {
            ProfileUpdate::AuthenticationRequired
        };
        let pending = PendingProfile {
            run_id,
            profile_id: snapshot.id,
            revision: snapshot.revision.clone(),
            update,
        };
        let mut file = NamedTempFile::new_in(self.root.join("pending"))?;
        file.write_all(&serde_json::to_vec(&pending).map_err(|_| ProfileError::State)?)?;
        file.as_file().sync_all()?;
        file.persist(self.root.join("pending").join(format!("{run_id}.json")))
            .map_err(|error| ProfileError::Io(error.error))?;
        fs::File::open(self.root.join("pending"))?.sync_all()?;
        Ok(())
    }

    pub fn publish(&self, run_id: Uuid, storage: &crate::storage::Storage) -> Result<()> {
        let _active = self.capture.lock().map_err(|_| ProfileError::Busy)?;
        let path = self.root.join("pending").join(format!("{run_id}.json"));
        if !path.exists() {
            return Ok(());
        }
        let pending: PendingProfile =
            serde_json::from_slice(&fs::read(&path)?).map_err(|_| ProfileError::State)?;
        let run = storage.get_run(run_id).map_err(|_| ProfileError::State)?;
        let current = fs::read(self.saved_path(pending.profile_id))?;
        if pending.run_id != run_id || run.job.profile_id != Some(pending.profile_id) {
            return Err(ProfileError::State);
        }
        if hex::encode(Sha256::digest(&current)) == pending.revision {
            match pending.update {
                ProfileUpdate::Session(saved)
                    if run.status == crate::storage::RunStatus::Succeeded
                        && saved.profile.id == pending.profile_id =>
                {
                    self.write_saved(&saved)?
                }
                ProfileUpdate::AuthenticationRequired
                    if run
                        .failure
                        .is_some_and(|failure| failure.code == "AUTHENTICATION_REQUIRED") =>
                {
                    let mut file = NamedTempFile::new_in(self.root.join("refresh"))?;
                    file.write_all(pending.revision.as_bytes())?;
                    file.as_file().sync_all()?;
                    file.persist(
                        self.root
                            .join("refresh")
                            .join(pending.profile_id.to_string()),
                    )
                    .map_err(|error| ProfileError::Io(error.error))?;
                    fs::File::open(self.root.join("refresh"))?.sync_all()?;
                }
                _ => {}
            }
        }
        fs::remove_file(path)?;
        fs::File::open(self.root.join("pending"))?.sync_all()?;
        Ok(())
    }

    pub fn recover(&self, storage: &crate::storage::Storage) -> Result<()> {
        for entry in fs::read_dir(self.root.join("pending"))? {
            let path = entry?.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                let id = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .and_then(|stem| Uuid::parse_str(stem).ok())
                    .ok_or(ProfileError::State)?;
                self.publish(id, storage)?;
            }
        }
        Ok(())
    }

    pub fn restore(&self, profile_id: Uuid) -> Result<Zeroizing<Vec<u8>>> {
        let _active = self.capture.lock().map_err(|_| ProfileError::Busy)?;
        self.restore_state(profile_id)
    }

    fn restore_state(&self, profile_id: Uuid) -> Result<Zeroizing<Vec<u8>>> {
        let path = self.root.join("saved").join(format!("{profile_id}.json"));
        let saved: SavedProfile =
            serde_json::from_slice(&fs::read(path)?).map_err(|_| ProfileError::State)?;
        let key = self.key.load(false)?;
        let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| ProfileError::Encryption)?;
        let nonce: [u8; 12] = saved
            .nonce
            .try_into()
            .map_err(|_| ProfileError::Encryption)?;
        cipher
            .decrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: &saved.ciphertext,
                    aad: profile_id.as_bytes(),
                },
            )
            .map(Zeroizing::new)
            .map_err(|_| ProfileError::Encryption)
    }
}

fn private_directory(path: &Path) -> Result<()> {
    DirBuilder::new().recursive(true).mode(0o700).create(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    struct TestKey(Arc<AtomicBool>);

    impl ProfileKey for TestKey {
        fn load(&self, _create: bool) -> Result<Zeroizing<Vec<u8>>> {
            if self.0.load(Ordering::Relaxed) {
                return Err(ProfileError::Keychain);
            }
            Ok(Zeroizing::new(vec![42; 32]))
        }
    }

    struct TestCredential(String);
    impl Drop for TestCredential {
        fn drop(&mut self) {
            if let Ok(entry) = keyring::Entry::new(&self.0, "encryption-key") {
                let _ = entry.delete_credential();
            }
        }
    }
    fn fixture(root: &Path, denied: Arc<AtomicBool>) -> Profiles {
        let mut profiles = Profiles::open(
            root.into(),
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/runner"),
        )
        .unwrap();
        profiles.key = Box::new(TestKey(denied));
        profiles
    }

    fn profile() -> SessionProfile {
        SessionProfile {
            id: Uuid::now_v7(),
            name: "Demo".into(),
            login_url: "https://example.com/login".into(),
            browser: "/Applications/Helium.app/Contents/MacOS/Helium".into(),
            captured_at: 0,
            needs_refresh: false,
        }
    }

    const STATE: &[u8] =
        br#"{"cookies":[{"name":"session","value":"private-session-token"}],"origins":[]}"#;

    #[test]
    fn encrypted_profile_survives_restart_and_rejects_tampering() {
        let directory = tempfile::tempdir().unwrap();
        let denied = Arc::new(AtomicBool::new(false));
        let profiles = fixture(directory.path(), denied.clone());
        let saved = profiles.save(profile(), STATE).unwrap();
        let path = directory
            .path()
            .join("saved")
            .join(format!("{}.json", saved.id));
        let bytes = fs::read(&path).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("private-session-token"));
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        drop(profiles);
        let profiles = fixture(directory.path(), denied.clone());
        assert_eq!(profiles.list().unwrap(), vec![saved.clone()]);
        assert_eq!(profiles.restore(saved.id).unwrap().as_slice(), STATE);
        denied.store(true, Ordering::Relaxed);
        assert!(matches!(
            profiles.restore(saved.id),
            Err(ProfileError::Keychain)
        ));
        assert!(matches!(
            profiles.save(saved.clone(), STATE),
            Err(ProfileError::Keychain)
        ));
        assert_eq!(fs::read(&path).unwrap(), bytes);
        denied.store(false, Ordering::Relaxed);
        let mut envelope: SavedProfile = serde_json::from_slice(&bytes).unwrap();
        envelope.ciphertext[0] ^= 1;
        fs::write(path, serde_json::to_vec(&envelope).unwrap()).unwrap();
        assert!(matches!(
            profiles.restore(saved.id),
            Err(ProfileError::Encryption)
        ));
    }

    #[test]
    fn failed_or_cancelled_refresh_preserves_saved_state_and_cleans_temporary_files() {
        let directory = tempfile::tempdir().unwrap();
        let profiles = fixture(directory.path(), Arc::new(AtomicBool::new(false)));
        let saved = profiles.save(profile(), STATE).unwrap();
        assert!(
            profiles
                .begin(
                    Some(saved.id),
                    "Demo".into(),
                    saved.login_url.clone(),
                    "/missing/Helium".into(),
                    Path::new("/usr/bin/true")
                )
                .is_err()
        );
        let temporary = tempfile::tempdir_in(directory.path().join("capture")).unwrap();
        let temporary_path = temporary.path().to_path_buf();
        fs::write(temporary.path().join("state.json"), STATE).unwrap();
        let status = CaptureStatus {
            id: Uuid::now_v7(),
            name: "Demo".into(),
            refreshing: true,
        };
        *profiles.capture.lock().unwrap() = Some(Capture {
            status: status.clone(),
            profile: saved.clone(),
            directory: temporary,
            child: Command::new("/bin/cat")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .spawn()
                .unwrap(),
        });
        assert!(matches!(
            profiles.cancel(Some(Uuid::now_v7())),
            Err(ProfileError::NotFound)
        ));
        assert!(profiles.status().unwrap().is_some());
        profiles.cancel(Some(status.id)).unwrap();
        assert!(!temporary_path.exists());
        assert_eq!(profiles.restore(saved.id).unwrap().as_slice(), STATE);
        assert!(profiles.status().unwrap().is_none());
        fs::create_dir(directory.path().join("capture/stale")).unwrap();
        fs::write(directory.path().join("capture/stale/state.json"), STATE).unwrap();
        drop(profiles);
        let profiles = fixture(directory.path(), Arc::new(AtomicBool::new(false)));
        assert_eq!(
            fs::read_dir(directory.path().join("capture"))
                .unwrap()
                .count(),
            0
        );
        assert_eq!(profiles.restore(saved.id).unwrap().as_slice(), STATE);
    }

    #[test]
    fn invalid_capture_never_replaces_a_saved_session() {
        let directory = tempfile::tempdir().unwrap();
        let profiles = fixture(directory.path(), Arc::new(AtomicBool::new(false)));
        let saved = profiles.save(profile(), STATE).unwrap();
        let temporary = tempfile::tempdir_in(directory.path().join("capture")).unwrap();
        let temporary_path = temporary.path().to_path_buf();
        fs::write(temporary.path().join("state.json"), b"invalid-state").unwrap();
        let status = CaptureStatus {
            id: Uuid::now_v7(),
            name: saved.name.clone(),
            refreshing: true,
        };
        *profiles.capture.lock().unwrap() = Some(Capture {
            status: status.clone(),
            profile: saved.clone(),
            directory: temporary,
            child: Command::new("/bin/sh")
                .arg("-c")
                .arg("read instruction")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .spawn()
                .unwrap(),
        });
        assert!(matches!(
            profiles.finish(status.id),
            Err(ProfileError::State)
        ));
        assert!(!temporary_path.exists());
        assert!(profiles.status().unwrap().is_none());
        assert_eq!(profiles.restore(saved.id).unwrap().as_slice(), STATE);
    }

    #[test]
    fn refresh_replaces_only_the_selected_profile() {
        let directory = tempfile::tempdir().unwrap();
        let profiles = fixture(directory.path(), Arc::new(AtomicBool::new(false)));
        let first = profiles.save(profile(), STATE).unwrap();
        let second = profiles.save(profile(), STATE).unwrap();
        let updated = br#"{"cookies":[],"origins":[{"origin":"https://example.com","localStorage":[{"name":"token","value":"refreshed"}]}]}"#;
        profiles.save(first.clone(), updated).unwrap();
        assert_eq!(profiles.restore(first.id).unwrap().as_slice(), updated);
        assert_eq!(profiles.restore(second.id).unwrap().as_slice(), STATE);
        assert_eq!(profiles.list().unwrap().len(), 2);
    }

    #[test]
    fn native_keychain_encrypts_and_restores_after_restart() {
        if std::env::var("BAMBOO_KEYCHAIN_TEST").as_deref() != Ok("1") {
            return;
        }
        let credential = TestCredential(format!("com.bamboo.dev.profiles.test.{}", Uuid::now_v7()));
        let directory = tempfile::tempdir().unwrap();
        let resources = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/runner");
        let mut profiles = Profiles::open(directory.path().into(), resources.clone()).unwrap();
        profiles.key = Box::new(KeychainKey {
            service: credential.0.clone(),
        });
        let saved = profiles.save(profile(), STATE).unwrap();
        drop(profiles);
        let mut profiles = Profiles::open(directory.path().into(), resources).unwrap();
        profiles.key = Box::new(KeychainKey {
            service: credential.0.clone(),
        });
        assert_eq!(profiles.restore(saved.id).unwrap().as_slice(), STATE);
    }

    #[test]
    fn real_helium_capture_persists_and_restores() {
        if std::env::var("BAMBOO_PROFILE_TEST").as_deref() != Ok("1") {
            return;
        }
        use std::{io::Read, net::TcpListener};
        fn login_fixture(token: &'static str) -> (String, thread::JoinHandle<()>) {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = format!("http://{}", listener.local_addr().unwrap());
            let server = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 4096];
                let _ = stream.read(&mut request);
                let body =
                    format!("<script>localStorage.setItem('login','{token}')</script>Signed in");
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nSet-Cookie: session={token}; Path=/; HttpOnly\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            });
            (address, server)
        }
        let (address, server) = login_fixture("synthetic-first");
        let directory = tempfile::tempdir().unwrap();
        let denied = Arc::new(AtomicBool::new(false));
        let profiles = fixture(directory.path(), denied.clone());
        let node = std::env::var("BAMBOO_NODE_EXECUTABLE").unwrap();
        let status = profiles
            .begin(
                None,
                "Local fixture".into(),
                address,
                "/Applications/Helium.app/Contents/MacOS/Helium".into(),
                Path::new(&node),
            )
            .unwrap();
        assert!(profiles.list().unwrap().is_empty());
        let saved = profiles.finish(status.id).unwrap();
        server.join().unwrap();
        assert!(profiles.status().unwrap().is_none());
        assert_eq!(
            fs::read_dir(directory.path().join("capture"))
                .unwrap()
                .count(),
            0
        );
        drop(profiles);
        let profiles = fixture(directory.path(), denied);
        let restored = profiles.restore(saved.id).unwrap();
        let state: serde_json::Value = serde_json::from_slice(&restored).unwrap();
        assert_eq!(state["cookies"][0]["value"], "synthetic-first");
        assert_eq!(
            state["origins"][0]["localStorage"][0]["value"],
            "synthetic-first"
        );
        let (address, server) = login_fixture("synthetic-cancelled");
        let status = profiles
            .begin(
                Some(saved.id),
                saved.name.clone(),
                address,
                saved.browser.clone(),
                Path::new(&node),
            )
            .unwrap();
        assert!(status.refreshing);
        profiles.cancel(Some(status.id)).unwrap();
        server.join().unwrap();
        assert_eq!(
            profiles.restore(saved.id).unwrap().as_slice(),
            restored.as_slice()
        );
        let (address, server) = login_fixture("synthetic-abandoned");
        let _status = profiles
            .begin(
                Some(saved.id),
                saved.name.clone(),
                address,
                saved.browser.clone(),
                Path::new(&node),
            )
            .unwrap();
        {
            let mut active = profiles.capture.lock().unwrap();
            let capture = active.as_mut().unwrap();
            capture.child.stdin.take();
            let deadline = Instant::now() + Duration::from_secs(8);
            while capture.child.try_wait().unwrap().is_none() {
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(20));
            }
            assert!(!capture.directory.path().join("browser").exists());
            assert!(!capture.directory.path().join("state.json").exists());
        }
        server.join().unwrap();
        assert!(matches!(profiles.status(), Err(ProfileError::Browser)));
        assert_eq!(
            profiles.restore(saved.id).unwrap().as_slice(),
            restored.as_slice()
        );
        let (address, server) = login_fixture("synthetic-refreshed");
        let status = profiles
            .begin(
                Some(saved.id),
                saved.name.clone(),
                address,
                saved.browser.clone(),
                Path::new(&node),
            )
            .unwrap();
        let refreshed = profiles.finish(status.id).unwrap();
        server.join().unwrap();
        assert_eq!(refreshed.id, saved.id);
        assert_eq!(profiles.list().unwrap().len(), 1);
        let state: serde_json::Value =
            serde_json::from_slice(&profiles.restore(saved.id).unwrap()).unwrap();
        assert_eq!(state["cookies"][0]["value"], "synthetic-refreshed");
        assert_eq!(
            state["origins"][0]["localStorage"][0]["value"],
            "synthetic-refreshed"
        );
        assert_eq!(
            fs::read_dir(directory.path().join("capture"))
                .unwrap()
                .count(),
            0
        );
    }
    fn queued_profile_run(
        storage: &crate::storage::Storage,
        root: &Path,
        profile_id: Uuid,
    ) -> crate::storage::Run {
        let entry = root.join(format!("job-{}.ts", Uuid::now_v7()));
        fs::write(&entry, "export default async () => []").unwrap();
        let job = storage
            .create_job(
                "Profile job",
                root,
                &entry,
                &serde_json::Value::Null,
                Duration::from_secs(20),
            )
            .unwrap();
        storage.set_job_profile(job.id, Some(profile_id)).unwrap();
        let queued = storage.queue_run(job.id, &serde_json::Value::Null).unwrap();
        storage
            .start_run(
                queued.id,
                crate::storage::RuntimeVersions {
                    node: "26".into(),
                    lightpanda: "fixture".into(),
                    playwright: "fixture".into(),
                },
            )
            .unwrap();
        queued
    }

    #[test]
    fn journal_recovery_requires_published_results_and_preserves_manual_refresh() {
        use crate::storage::{RunCompletion, Storage};
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let storage = Storage::open(&root.join("storage")).unwrap();
        let denied = Arc::new(AtomicBool::new(false));
        let profiles = fixture(&root.join("profiles"), denied.clone());
        let saved = profiles.save(profile(), STATE).unwrap();
        let updated = br#"{"cookies":[],"origins":[]}"#;
        let queued = queued_profile_run(&storage, root, saved.id);
        let snapshot = profiles.snapshot(saved.id).unwrap();
        profiles.stage(queued.id, &snapshot, Some(updated)).unwrap();
        assert_eq!(profiles.restore(saved.id).unwrap().as_slice(), STATE);
        storage
            .finish_run(queued.id, RunCompletion::Succeeded(vec![]))
            .unwrap();
        drop(profiles);
        let profiles = fixture(&root.join("profiles"), denied.clone());
        profiles.recover(&storage).unwrap();
        assert_eq!(profiles.restore(saved.id).unwrap().as_slice(), updated);
        let bytes = fs::read(profiles.saved_path(saved.id)).unwrap();
        profiles.recover(&storage).unwrap();
        assert_eq!(fs::read(profiles.saved_path(saved.id)).unwrap(), bytes);
        let queued = queued_profile_run(&storage, root, saved.id);
        profiles
            .stage(
                queued.id,
                &profiles.snapshot(saved.id).unwrap(),
                Some(STATE),
            )
            .unwrap();
        storage.recover_unfinished_runs().unwrap();
        profiles.recover(&storage).unwrap();
        assert_eq!(profiles.restore(saved.id).unwrap().as_slice(), updated);
        let queued = queued_profile_run(&storage, root, saved.id);
        profiles
            .stage(
                queued.id,
                &profiles.snapshot(saved.id).unwrap(),
                Some(STATE),
            )
            .unwrap();
        let manual =
            br#"{"cookies":[],"origins":[{"origin":"https://example.com","localStorage":[] }]}"#;
        profiles.save(saved.clone(), manual).unwrap();
        storage
            .finish_run(queued.id, RunCompletion::Succeeded(vec![]))
            .unwrap();
        profiles.recover(&storage).unwrap();
        assert_eq!(profiles.restore(saved.id).unwrap().as_slice(), manual);
        assert_eq!(
            fs::read_dir(root.join("profiles/pending")).unwrap().count(),
            0
        );
    }

    #[test]
    fn failed_publication_and_authentication_failure_have_distinct_profile_outcomes() {
        use crate::storage::{RunCompletion, RunFailure, Storage};
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let storage = Storage::open(&root.join("storage")).unwrap();
        let profiles = fixture(&root.join("profiles"), Arc::new(AtomicBool::new(false)));
        let saved = profiles.save(profile(), STATE).unwrap();
        let queued = queued_profile_run(&storage, root, saved.id);
        profiles
            .stage(
                queued.id,
                &profiles.snapshot(saved.id).unwrap(),
                Some(br#"{"cookies":[],"origins":[]}"#),
            )
            .unwrap();
        fs::create_dir(
            root.join("storage/runs")
                .join(queued.id.to_string())
                .join("results.json"),
        )
        .unwrap();
        assert!(
            storage
                .finish_run(queued.id, RunCompletion::Succeeded(vec![]))
                .is_err()
        );
        fs::remove_dir(
            root.join("storage/runs")
                .join(queued.id.to_string())
                .join("results.json"),
        )
        .unwrap();
        storage.recover_unfinished_runs().unwrap();
        profiles.recover(&storage).unwrap();
        assert_eq!(profiles.restore(saved.id).unwrap().as_slice(), STATE);
        assert!(!profiles.list().unwrap()[0].needs_refresh);
        let queued = queued_profile_run(&storage, root, saved.id);
        profiles
            .stage(queued.id, &profiles.snapshot(saved.id).unwrap(), None)
            .unwrap();
        storage
            .finish_run(
                queued.id,
                RunCompletion::Failed(RunFailure {
                    code: "AUTHENTICATION_REQUIRED".into(),
                    message: "Refresh login".into(),
                }),
            )
            .unwrap();
        profiles.recover(&storage).unwrap();
        assert!(profiles.list().unwrap()[0].needs_refresh);
        assert!(storage.read_results(queued.id).is_err());
        assert_eq!(profiles.restore(saved.id).unwrap().as_slice(), STATE);
        profiles.save(saved, STATE).unwrap();
        assert!(!profiles.list().unwrap()[0].needs_refresh);
    }

    #[tokio::test]
    async fn real_profile_runs_rotate_only_after_success_and_signal_expiry() {
        if std::env::var("BAMBOO_PROFILE_RUN_TEST").as_deref() != Ok("1") {
            return;
        }
        use crate::{
            runner::{RunSupervisor, RuntimePaths},
            storage::{RunStatus, Storage},
        };
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let storage = Storage::open(&root.join("storage")).unwrap();
        let profiles = Arc::new(fixture(
            &root.join("profiles"),
            Arc::new(AtomicBool::new(false)),
        ));
        let initial = serde_json::to_vec(&serde_json::json!({ "cookies": [{ "name": "session", "value": "synthetic-original", "domain": "example.com", "path": "/", "expires": -1, "httpOnly": true, "secure": false, "sameSite": "Lax" }], "origins": [] })).unwrap();
        let saved = profiles.save(profile(), &initial).unwrap();
        let resources = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/runner");
        let supervisor = Arc::new(
            RunSupervisor::new(
                storage.clone(),
                resources.clone(),
                root.join("configuration"),
                profiles.clone(),
            )
            .unwrap(),
        );
        supervisor
            .configure_runtimes(RuntimePaths {
                node: std::env::var("BAMBOO_NODE_EXECUTABLE").unwrap().into(),
                lightpanda: std::env::var("BAMBOO_LIGHTPANDA_EXECUTABLE")
                    .unwrap()
                    .into(),
            })
            .unwrap();
        let entry = root.join("job.ts");
        fs::write(root.join("package.json"), "{\"type\":\"module\"}").unwrap();
        fs::write(&entry, "export default async () => []").unwrap();
        let job = storage
            .create_job(
                "Session rotation",
                root,
                &entry,
                &serde_json::Value::Null,
                Duration::from_secs(30),
            )
            .unwrap();
        storage.set_job_profile(job.id, Some(saved.id)).unwrap();
        let rotation = "const cookies = await context.cookies(); if (cookies[0]?.value !== 'synthetic-original') throw new Error('Session was not restored'); await context.addCookies([{...cookies[0],value:'synthetic-rotated'}]);";
        fs::write(&entry, format!("export default async function job({{context}}) {{ {rotation} return [{{authenticated:true}}] }}")).unwrap();
        let run = supervisor
            .run(storage.queue_run(job.id, &serde_json::Value::Null).unwrap())
            .await
            .unwrap();
        assert_eq!(run.status, RunStatus::Succeeded);
        let state = profiles.restore(saved.id).unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&state).unwrap();
        assert_eq!(parsed["cookies"][0]["value"], "synthetic-rotated");
        for ending in [
            "throw new Error('Ordinary failure')",
            "return [{bad:undefined}]",
        ] {
            fs::write(&entry, format!("export default async function job({{context}}) {{ await context.clearCookies(); {ending} }}")).unwrap();
            let run = supervisor
                .run(storage.queue_run(job.id, &serde_json::Value::Null).unwrap())
                .await
                .unwrap();
            assert_eq!(run.status, RunStatus::Failed);
            assert_eq!(
                profiles.restore(saved.id).unwrap().as_slice(),
                state.as_slice()
            );
            assert!(!profiles.list().unwrap()[0].needs_refresh);
            assert!(storage.read_results(run.id).is_err());
        }
        let signal = root.join("job-started");
        fs::write(&entry, format!("import {{ writeFile }} from 'node:fs/promises'; export default async function job({{context}}) {{ await context.clearCookies(); await writeFile({},'ready'); await new Promise(() => {{}}) }}", serde_json::to_string(&signal).unwrap())).unwrap();
        let queued = storage.queue_run(job.id, &serde_json::Value::Null).unwrap();
        let id = queued.id;
        let running = supervisor.clone();
        let task = tokio::spawn(async move { running.run(queued).await });
        let deadline = Instant::now() + Duration::from_secs(20);
        while !signal.exists() {
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(supervisor.cancel(id));
        assert_eq!(task.await.unwrap().unwrap().status, RunStatus::Cancelled);
        assert_eq!(
            profiles.restore(saved.id).unwrap().as_slice(),
            state.as_slice()
        );
        let auth_module = resources.join("job.ts");
        fs::write(&entry, format!("import {{ AuthenticationRequiredError }} from {}; export default async function job({{context}}) {{ await context.clearCookies(); throw new AuthenticationRequiredError() }}", serde_json::to_string(&auth_module).unwrap())).unwrap();
        let run = supervisor
            .run(storage.queue_run(job.id, &serde_json::Value::Null).unwrap())
            .await
            .unwrap();
        assert_eq!(run.failure.unwrap().code, "AUTHENTICATION_REQUIRED");
        assert!(profiles.list().unwrap()[0].needs_refresh);
        assert_eq!(
            profiles.restore(saved.id).unwrap().as_slice(),
            state.as_slice()
        );
        assert!(storage.read_results(run.id).is_err());
        for entry in fs::read_dir(root.join("storage/runs").join(run.id.to_string())).unwrap() {
            assert!(
                !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("protocol-")
            );
        }
        for stream in [
            crate::storage::LogStream::Stdout,
            crate::storage::LogStream::Stderr,
        ] {
            let logs = storage.read_log(run.id, stream).unwrap();
            assert!(!String::from_utf8_lossy(&logs).contains("synthetic-rotated"));
        }
        profiles
            .save(
                saved.clone(),
                br#"{"cookies":[{"value":"synthetic-invalid-secret"}],"origins":[]}"#,
            )
            .unwrap();
        let run = supervisor
            .run(storage.queue_run(job.id, &serde_json::Value::Null).unwrap())
            .await
            .unwrap();
        assert_eq!(run.failure.unwrap().code, "PROFILE_STATE");
        assert!(!profiles.list().unwrap()[0].needs_refresh);
        let logs = storage
            .read_log(run.id, crate::storage::LogStream::Stderr)
            .unwrap();
        assert!(!String::from_utf8_lossy(&logs).contains("synthetic-invalid-secret"));
    }
    #[tokio::test]
    async fn deleting_history_finishes_pending_profile_publication_without_orphaning_recovery() {
        use crate::{
            queue::RunQueue,
            runner::RunSupervisor,
            storage::{HistoryTarget, RunCompletion, Storage},
        };
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let storage = Storage::open(&root.join("storage")).unwrap();
        let profiles = Arc::new(fixture(
            &root.join("profiles"),
            Arc::new(AtomicBool::new(false)),
        ));
        let saved = profiles.save(profile(), STATE).unwrap();
        let queued = queued_profile_run(&storage, root, saved.id);
        let updated = br#"{"cookies":[],"origins":[]}"#;
        profiles
            .stage(
                queued.id,
                &profiles.snapshot(saved.id).unwrap(),
                Some(updated),
            )
            .unwrap();
        storage
            .finish_run(queued.id, RunCompletion::Succeeded(vec![]))
            .unwrap();
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/runner");
        let supervisor = Arc::new(
            RunSupervisor::new(
                storage.clone(),
                assets,
                root.join("config"),
                profiles.clone(),
            )
            .unwrap(),
        );
        let queue = RunQueue::new(storage.clone(), supervisor);
        let deleted = queue
            .delete_history(HistoryTarget::Run(queued.id))
            .await
            .unwrap();
        assert_eq!(deleted.deleted, vec![queued.id]);
        assert_eq!(profiles.restore(saved.id).unwrap().as_slice(), updated);
        profiles.recover(&storage).unwrap();
        assert_eq!(
            storage.get_job(queued.job.id).unwrap().profile_id,
            Some(saved.id)
        );
        queue.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn authenticated_helium_profile_runs_on_schedule_exports_and_survives_history_deletion() {
        if std::env::var("BAMBOO_ACCEPTANCE_TEST").as_deref() != Ok("1") {
            return;
        }
        use crate::{
            queue::{RunQueue, ScheduleDispatchDecision},
            results::{self, ExportFormat},
            runner::{RunSupervisor, RuntimePaths},
            schedule::{Clock, ClockReading, IntervalUnit, Schedule, ScheduleError},
            scheduler::Scheduler,
            storage::{HistoryTarget, RunStatus, Storage},
        };
        use std::{io::Read, net::TcpListener};
        struct Site {
            stop: Arc<AtomicBool>,
            worker: Option<thread::JoinHandle<()>>,
        }
        impl Drop for Site {
            fn drop(&mut self) {
                self.stop.store(true, Ordering::SeqCst);
                if let Some(worker) = self.worker.take() {
                    let _ = worker.join();
                }
            }
        }
        struct AcceptanceClock(Mutex<jiff::Timestamp>);
        impl Clock for AcceptanceClock {
            fn read(&self) -> std::result::Result<ClockReading, ScheduleError> {
                Ok(ClockReading {
                    now: *self.0.lock().unwrap(),
                    time_zone: jiff::tz::TimeZone::UTC,
                })
            }
        }
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let protected_requests = Arc::new(Mutex::new(Vec::new()));
        let observed_requests = protected_requests.clone();
        let worker = thread::spawn(move || {
            while !worker_stop.load(Ordering::SeqCst) {
                let Ok((mut stream, _)) = listener.accept() else {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut chunk = [0; 1024];
                while bytes.len() < 8192 && !bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                    match stream.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(length) => bytes.extend_from_slice(&chunk[..length]),
                    }
                }
                let request = String::from_utf8_lossy(&bytes);
                let login = request.starts_with("GET /login ");
                let authenticated = request.contains("session=synthetic-login")
                    || request.contains("session=synthetic-rotated");
                if request.starts_with("GET /protected ") {
                    observed_requests.lock().unwrap().push(authenticated);
                }
                let (status, body, cookie) = if login {
                    (
                        "200 OK",
                        "<script>localStorage.setItem('fixture','ready')</script><h1>Logged in</h1>",
                        "Set-Cookie: session=synthetic-login; Path=/; HttpOnly; SameSite=Lax\r\n",
                    )
                } else if authenticated && request.starts_with("GET /protected ") {
                    (
                        "200 OK",
                        "<h1>Authenticated</h1>",
                        "Set-Cookie: session=synthetic-rotated; Path=/; HttpOnly; SameSite=Lax\r\n",
                    )
                } else {
                    ("401 Unauthorized", "<h1>Login required</h1>", "")
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: text/html\r\n{cookie}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        let _site = Site {
            stop,
            worker: Some(worker),
        };
        let credential = TestCredential(format!(
            "com.bamboo.dev.profiles.acceptance.{}",
            Uuid::now_v7()
        ));
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let resources = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/runner");
        let mut profiles = Profiles::open(root.join("profiles"), resources.clone()).unwrap();
        profiles.key = Box::new(KeychainKey {
            service: credential.0.clone(),
        });
        let node = PathBuf::from(std::env::var("BAMBOO_NODE_EXECUTABLE").unwrap());
        let capture = profiles
            .begin(
                None,
                "Acceptance fixture".into(),
                format!("{address}/login"),
                "/Applications/Helium.app/Contents/MacOS/Helium".into(),
                &node,
            )
            .unwrap();
        let saved = profiles.finish(capture.id).unwrap();
        let captured: serde_json::Value =
            serde_json::from_slice(&profiles.restore(saved.id).unwrap()).unwrap();
        assert_eq!(captured["cookies"][0]["name"], "session");
        assert_eq!(captured["cookies"][0]["value"], "synthetic-login");
        let profiles = Arc::new(profiles);
        let storage = Storage::open(&root.join("storage")).unwrap();
        let entry = root.join("job.ts");
        fs::write(root.join("package.json"), "{\"type\":\"module\"}").unwrap();
        fs::write(&entry, format!("import {{AuthenticationRequiredError}} from {}; export default async function job({{context,input}}) {{ const page=await context.newPage(); const response=await page.goto({}); if(!response) throw new Error('ACCEPTANCE_NO_RESPONSE'); if(response.status() !== 200) throw new AuthenticationRequiredError(); return [{{authenticated:await page.locator('h1').innerText()==='Authenticated',label:input.label}}] }}", serde_json::to_string(&resources.join("job.ts")).unwrap(), serde_json::to_string(&format!("{address}/protected")).unwrap())).unwrap();
        let job = storage
            .create_job(
                "Authenticated schedule",
                root,
                &entry,
                &serde_json::json!({"label":"saved schedule input"}),
                Duration::from_secs(30),
            )
            .unwrap();
        let job = storage.set_job_profile(job.id, Some(saved.id)).unwrap();
        let clock = Arc::new(AcceptanceClock(Mutex::new(
            "2026-10-27T08:00:00Z".parse().unwrap(),
        )));
        storage
            .save_schedule(
                job.id,
                Schedule::Interval {
                    every: 1,
                    unit: IntervalUnit::Minutes,
                },
                &*clock,
            )
            .unwrap();
        let supervisor = Arc::new(
            RunSupervisor::new(
                storage.clone(),
                resources.clone(),
                root.join("config"),
                profiles.clone(),
            )
            .unwrap(),
        );
        supervisor
            .configure_runtimes(RuntimePaths {
                node,
                lightpanda: std::env::var("BAMBOO_LIGHTPANDA_EXECUTABLE")
                    .unwrap()
                    .into(),
            })
            .unwrap();
        let queue = RunQueue::new(storage.clone(), supervisor);
        let scheduler = Scheduler::new(storage.clone(), queue.clone(), clock.clone());
        let start = Instant::now();
        scheduler.tick(start).await.unwrap();
        *clock.0.lock().unwrap() = "2026-10-27T08:00:59Z".parse().unwrap();
        scheduler
            .tick(start + Duration::from_secs(59))
            .await
            .unwrap();
        *clock.0.lock().unwrap() = "2026-10-27T08:01:00Z".parse().unwrap();
        let events = scheduler
            .tick(start + Duration::from_secs(60))
            .await
            .unwrap();
        let ScheduleDispatchDecision::Launched { run_id } = events[0].decision else {
            panic!("Expected authenticated scheduled run")
        };
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if queue.snapshot().await.unwrap().active.is_none() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        let completed = storage.get_run(run_id).unwrap();
        assert_eq!(
            completed.status,
            RunStatus::Succeeded,
            "{:?}; protected request auth flags: {:?}",
            completed.failure,
            protected_requests.lock().unwrap()
        );
        let manual = queue
            .enqueue(job.id, serde_json::json!({"label":"manual override"}))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let snapshot = queue.snapshot().await.unwrap();
                if snapshot.active.is_none() && snapshot.pending.is_empty() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            storage.read_results(manual.id).unwrap()[0]["label"],
            "manual override"
        );
        assert_eq!(storage.get_job(job.id).unwrap().input, job.input);

        scheduler.shutdown().await;
        queue.shutdown().await.unwrap();
        let reopened = Storage::open(&root.join("storage")).unwrap();
        reopened.recover_unfinished_runs().unwrap();
        let mut restored_profiles = Profiles::open(root.join("profiles"), resources).unwrap();
        restored_profiles.key = Box::new(KeychainKey {
            service: credential.0.clone(),
        });
        let state: serde_json::Value =
            serde_json::from_slice(&restored_profiles.restore(saved.id).unwrap()).unwrap();
        assert_eq!(state["cookies"][0]["value"], "synthetic-rotated");
        let json = results::export_bytes(&reopened, run_id, ExportFormat::Json).unwrap();
        let csv = results::export_bytes(&reopened, run_id, ExportFormat::Csv).unwrap();
        results::save_export(&root.join("export.json"), &json).unwrap();
        results::save_export(&root.join("export.csv"), &csv).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(
                &fs::read(root.join("export.json")).unwrap()
            )
            .unwrap(),
            serde_json::json!([{"authenticated":true,"label":"saved schedule input"}])
        );
        let mut reader = csv::Reader::from_reader(csv.as_slice());
        assert_eq!(
            reader
                .records()
                .next()
                .unwrap()
                .unwrap()
                .iter()
                .collect::<Vec<_>>(),
            vec!["true", "saved schedule input"]
        );
        assert_eq!(
            queue
                .delete_history(HistoryTarget::Run(run_id))
                .await
                .unwrap()
                .deleted,
            vec![run_id]
        );
        assert_eq!(reopened.list_runs(job.id, 20, 0).unwrap().len(), 1);
        assert_eq!(
            queue
                .delete_history(HistoryTarget::Job(job.id))
                .await
                .unwrap()
                .deleted,
            vec![manual.id]
        );
        assert!(reopened.list_runs(job.id, 20, 0).unwrap().is_empty());
        assert!(root.join("export.json").exists());
        assert!(root.join("export.csv").exists());
        assert_eq!(reopened.get_job(job.id).unwrap(), job);
        assert_eq!(restored_profiles.list().unwrap()[0].id, saved.id);
        assert!(reopened.get_schedule(job.id).unwrap().is_some());
        assert!(
            scheduler
                .tick(start + Duration::from_secs(120))
                .await
                .unwrap()
                .is_empty()
        );
    }
}
