import * as Sentry from '@sentry/cloudflare';

export default (nitroApp: any) => {
  let initialized = false;

  nitroApp.hooks.hook('request', () => {
    if (initialized) return;
    const dsn = (globalThis as any).__env__?.SENTRY_DSN ?? process.env.SENTRY_DSN;
    if (dsn) {
      const init = (Sentry as Record<string, unknown>)["init"];
      if (typeof init === "function") {
        init({ dsn, tracesSampleRate: 1.0 });
        initialized = true;
      }
    }
  });

  nitroApp.hooks.hook('error', async (error: any) => {
    console.error(error);
    Sentry.captureException(error);
    await Sentry.flush(2000);
  });
};
