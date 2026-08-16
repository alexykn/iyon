export type AuthCommand = "login" | "logout" | "status";
export type CliCommand = { readonly type: "run" } | { readonly type: "help" } | { readonly type: "auth"; readonly command: AuthCommand };

export class CliArgumentError extends Error {
  constructor(message: string) { super(message); this.name = "CliArgumentError"; }
}

export function parseArgs(argv: readonly string[] = process.argv.slice(2)): CliCommand {
  if (argv.length === 0 || argv[0] === "run") {
    if (argv.length > 1) throw new CliArgumentError(`unexpected argument: ${argv[1]}`);
    return { type: "run" };
  }
  if (argv[0] === "--help" || argv[0] === "-h" || argv[0] === "help") {
    if (argv.length > 1) throw new CliArgumentError(`unexpected argument: ${argv[1]}`);
    return { type: "help" };
  }
  if (argv[0] !== "auth") throw new CliArgumentError(`unknown command: ${argv[0]}`);
  const command = argv[1];
  if (command !== "login" && command !== "logout" && command !== "status") throw new CliArgumentError(command === undefined ? "auth requires login, logout, or status" : `unknown auth command: ${command}`);
  if (argv.length > 2) throw new CliArgumentError(`unexpected argument: ${argv[2]}`);
  return { type: "auth", command };
}
