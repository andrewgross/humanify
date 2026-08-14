import "./core/logger";
import { runAll, runLazy } from "./app/runner";
import env from "./config/env";
console.log("corpus-entry", env.name, runAll());
runLazy().then((r) => console.log("lazy:", r));
