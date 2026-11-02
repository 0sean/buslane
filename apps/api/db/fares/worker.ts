import { workerData } from "node:worker_threads";
import { openFaresReader, SCHEMA_VERSION } from "./index.ts";
import { FaresRepository } from "./repository.ts";
import { runWorker } from "../worker-runtime.ts";
const sqlite = openFaresReader(workerData.database);
runWorker(sqlite, new FaresRepository(sqlite), workerData, "Fares", SCHEMA_VERSION);
