import { workerData } from "node:worker_threads";
import { openReader, SCHEMA_VERSION } from "./timetables/index.ts";
import { Repository } from "./repository.ts";
import { runWorker } from "./worker-runtime.ts";
const { sqlite } = openReader(workerData.database);
runWorker(sqlite, new Repository(sqlite), workerData, "Timetable", SCHEMA_VERSION);
