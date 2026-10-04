import dep00 from "./vendor/mod00.cjs";
import dep01 from "./vendor/mod01.cjs";
import dep02 from "./vendor/mod02.cjs";
import dep03 from "./vendor/mod03.cjs";
import dep04 from "./vendor/mod04.cjs";
import dep05 from "./vendor/mod05.cjs";
import dep06 from "./vendor/mod06.cjs";
import dep07 from "./vendor/mod07.cjs";
import dep08 from "./vendor/mod08.cjs";
import dep09 from "./vendor/mod09.cjs";
import dep10 from "./vendor/mod10.cjs";
import dep11 from "./vendor/mod11.cjs";
import dep12 from "./vendor/mod12.cjs";
import dep13 from "./vendor/mod13.cjs";
import dep14 from "./vendor/mod14.cjs";
import dep15 from "./vendor/mod15.cjs";
import dep16 from "./vendor/mod16.cjs";
import dep17 from "./vendor/mod17.cjs";
import dep18 from "./vendor/mod18.cjs";
import dep19 from "./vendor/mod19.cjs";
import dep20 from "./vendor/mod20.cjs";
import dep21 from "./vendor/mod21.cjs";
import dep22 from "./vendor/mod22.cjs";
import dep23 from "./vendor/mod23.cjs";
import { pad } from "./util.js";
import ms from "ms";
import { basename } from "node:path";
import legacy from "./legacy.cjs";

async function main() {
  const { stamp } = await import("./lazy.js");
  const lines = [
    `dep00: ${dep00.tag()}${dep00.twice(21)}`,
    `dep01: ${dep01.tag()}${dep01.twice(21)}`,
    `dep02: ${dep02.tag()}${dep02.twice(21)}`,
    `dep03: ${dep03.tag()}${dep03.twice(21)}`,
    `dep04: ${dep04.tag()}${dep04.twice(21)}`,
    `dep05: ${dep05.tag()}${dep05.twice(21)}`,
    `dep06: ${dep06.tag()}${dep06.twice(21)}`,
    `dep07: ${dep07.tag()}${dep07.twice(21)}`,
    `dep08: ${dep08.tag()}${dep08.twice(21)}`,
    `dep09: ${dep09.tag()}${dep09.twice(21)}`,
    `dep10: ${dep10.tag()}${dep10.twice(21)}`,
    `dep11: ${dep11.tag()}${dep11.twice(21)}`,
    `dep12: ${dep12.tag()}${dep12.twice(21)}`,
    `dep13: ${dep13.tag()}${dep13.twice(21)}`,
    `dep14: ${dep14.tag()}${dep14.twice(21)}`,
    `dep15: ${dep15.tag()}${dep15.twice(21)}`,
    `dep16: ${dep16.tag()}${dep16.twice(21)}`,
    `dep17: ${dep17.tag()}${dep17.twice(21)}`,
    `dep18: ${dep18.tag()}${dep18.twice(21)}`,
    `dep19: ${dep19.tag()}${dep19.twice(21)}`,
    `dep20: ${dep20.tag()}${dep20.twice(21)}`,
    `dep21: ${dep21.tag()}${dep21.twice(21)}`,
    `dep22: ${dep22.tag()}${dep22.twice(21)}`,
    `dep23: ${dep23.tag()}${dep23.twice(21)}`,
    `pad: ${pad(3)}`,
    `ms: ${ms(90000)} ${ms("2h")}`,
    `path: ${basename("/var/log/report.txt")}`,
    `late: ${legacy.lateReport(21)}`,
    `stamp: ${stamp(7)}`
  ];
  for (const line of lines) {
    console.log(line);
  }
}
main();
