declare module "process" {
  /**
   * Standard input reader facade.
   */
  export interface StdReader {
    /**
     * Reads text from stdin.
     */
    read(maxBytes?: number): string;
  }

  /**
   * Standard output/error writer facade.
   */
  export interface StdWriter {
    /**
     * Writes text to target stream.
     */
    write(message: string): void;
  }

  /**
   * Grouped std handles exposed by the package.
   */
  export interface StdHandles {
    in: StdReader;
    out: StdWriter;
    err: StdWriter;
  }

  /**
   * Standard I/O handles implemented over RTS base APIs.
   */
  export const std: StdHandles;

  export function argv(): Array<string>;
  export function pwd(): string;
  export function getEnv(name: string): string | undefined;
  export function setEnv(name: string, value: string): void;
  export function getPid(): number;
  export function getPlatform(): string;
  export function getArch(): string;
  export function readStdin(maxBytes?: number): string;
  export function writeStdout(message: string): void;
  export function writeStderr(message: string): void;
  export function terminate(code?: number): never;
  export function delay(ms: number): void;
}
