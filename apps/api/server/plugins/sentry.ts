import * as Sentry from "@sentry/node";
export default (nitroApp: any) => {
  if (process.env.SENTRY_DSN)
    Sentry.init({ dsn: process.env.SENTRY_DSN, tracesSampleRate: 0.05 });
  nitroApp.hooks.hook("error", (error: Error) => {
    console.error(error);
    Sentry.captureException(error);
  });
  nitroApp.hooks.hook("close", () => Sentry.close(2000));
};
