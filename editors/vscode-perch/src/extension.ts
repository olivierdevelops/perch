// VS Code extension entry point — spawns `perch-lsp` as the language
// server for `.perch` files. Users need `perch-lsp` on their PATH
// (install via `go install github.com/luowensheng/perch/cmd/perch-lsp@latest`).
import * as path from "path";
import { ExtensionContext, workspace, window, commands } from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export function activate(context: ExtensionContext) {
  const cfg = workspace.getConfiguration("perch");
  const serverPath = cfg.get<string>("lsp.path") || "perch-lsp";

  const serverOptions: ServerOptions = {
    command: serverPath,
    args: [],
    options: {},
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "perch" }],
    synchronize: {
      fileEvents: workspace.createFileSystemWatcher("**/*.perch"),
    },
  };

  client = new LanguageClient(
    "perch",
    "perch language server",
    serverOptions,
    clientOptions,
  );

  client.start().catch((err) => {
    window.showWarningMessage(
      `perch: could not start language server (${serverPath}). ` +
        `Install with: go install github.com/luowensheng/perch/cmd/perch-lsp@latest. ` +
        `Error: ${err.message ?? err}`,
    );
  });

  context.subscriptions.push(
    commands.registerCommand("perch.restartServer", async () => {
      if (client) {
        await client.stop();
        await client.start();
        window.showInformationMessage("perch: language server restarted");
      }
    }),
  );
}

export function deactivate(): Thenable<void> | undefined {
  return client?.stop();
}
