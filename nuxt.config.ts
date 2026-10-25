import tailwindcss from '@tailwindcss/vite';
import * as v from 'valibot';

const DEFAULT_HOST = '0.0.0.0';
const DEFAULT_PORT = '3000';

const host = v.parse(v.optional(v.string(), DEFAULT_HOST), process.env.NUXT_HOST);
const port = v.parse(
  v.optional(v.pipe(v.string(), v.transform(Number), v.number()), DEFAULT_PORT),
  process.env.NUXT_PORT,
);

export default defineNuxtConfig({
  compatibilityDate: '2026-09-27',
  devtools: {
    enabled: true,
  },
  devServer: {
    host,
  },
  vite: {
    clearScreen: false,
    envPrefix: ['VITE_', 'TAURI_'],
    server: {
      strictPort: true,
      ws: {
        protocol: 'ws',
        port,
        host,
      },
      watch: {
        ignored: ['**/src-tauri/**'],
      },
    },
    optimizeDeps: {
      include: ['class-variance-authority', 'clsx', 'reka-ui', 'tailwind-merge'],
    },
    plugins: [tailwindcss()],
  },
  app: {
    pageTransition: {
      name: 'page',
      mode: 'out-in',
    },
    layoutTransition: {
      name: 'layout',
      mode: 'out-in',
    },
  },
  router: {
    options: {
      scrollBehaviorType: 'smooth',
    },
  },
  dir: {
    modules: 'app/modules',
  },
  ssr: false,
  ignore: ['**/src-tauri/**'],
  css: ['~/assets/css/tailwind.css'],
  modules: ['@nuxtjs/color-mode', '@vueuse/nuxt/module', 'shadcn-nuxt', '@nuxt/icon'],
  colorMode: {
    classSuffix: '',
    preference: 'dark',
  },
  shadcn: {
    prefix: '',
    componentDir: '~/components/ui',
  },
});
