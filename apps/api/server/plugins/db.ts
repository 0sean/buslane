import type { HTTPEvent } from 'h3';
import { createDb } from '~/db/timetables';

export default (nitroApp: any) => {
  nitroApp.hooks.hook('request', (event: HTTPEvent) => {
    const { env } = event.req.runtime!.cloudflare!;
    const hyperdrive = env.HYPERDRIVE as { connectionString: string; };
    const connectionString = hyperdrive?.connectionString ?? process.env.TIMETABLES_DATABASE_URL;
    if (connectionString) {
      (event.context as any).db = createDb(connectionString);
    }
  });
};
