import { register } from "./registry";

register("logger-loaded");

export class Logger {
  constructor(private tag: string) {}
  log(msg: string): void {
    console.log(`[${this.tag}] ${msg}`);
  }
}
export const rootLogger = new Logger("root");
