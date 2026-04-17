import { initDb } from '~/db/timetables';

export default (nitroApp: any) => {
  nitroApp.hooks.hook('request', () => {
    const hyperdrive = (globalThis as any).__env__?.HYPERDRIVE;
    const connectionString = hyperdrive?.connectionString ?? process.env.TIMETABLES_DATABASE_URL;
    if (connectionString) initDb(connectionString);
  });
};
