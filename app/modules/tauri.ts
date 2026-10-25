import * as tauriCli from '@tauri-apps/cli';
import { addImports, defineNuxtModule } from 'nuxt/kit';
import { capitalize } from '../utils/capitalize.ts';

type ModuleOptions = {
  prefix: false | string;
};

const tauriModules = [
  {
    module: tauriCli,
    prefix: 'Cli',
    importPath: '@tauri-apps/cli',
  },
];

export default defineNuxtModule<ModuleOptions>({
  meta: {
    name: 'nuxt-tauri',
    configKey: 'tauri',
  },
  defaults: {
    prefix: 'useTauri',
  },
  setup: (options) => {
    for (const { module, prefix, importPath: from } of tauriModules) {
      const as = `${options.prefix}${prefix}`;

      if ('default' in module) {
        addImports({
          name: 'default',
          from,
          as,
        });
      }

      for (const name of Object.keys(module)) {
        if (name === 'default' || name === 'module.exports') {
          continue;
        }

        addImports({
          name,
          from,
          as: as + capitalize(name),
        });
      }
    }
  },
});
