// VS Code extension entry point — spawns `perch-lsp` as the language
// server for `.perch` files. Plain JS so no TypeScript build is needed.
//
// Users only need:
//   1. `go install github.com/luowensheng/perch/cmd/perch-lsp@latest`
//   2. install this extension (`scripts/install-vscode.sh` does both
//       steps + `code --install-extension`).

const path = require("path");
const { workspace, window, commands } = require("vscode");
const { LanguageClient } = require("vscode-languageclient/node");

/** @type {import("vscode-languageclient/node").LanguageClient | undefined} */
let client;

function activate(context) {
    const cfg = workspace.getConfiguration("perch");
    const serverPath = cfg.get("lsp.path") || "perch-lsp";

    const serverOptions = {
        command: serverPath,
        args: [],
        options: {},
    };

    const clientOptions = {
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
            "perch: could not start " + serverPath + ". " +
            "Install with: go install github.com/luowensheng/perch/cmd/perch-lsp@latest. " +
            "Error: " + (err && err.message ? err.message : String(err)),
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

function deactivate() {
    return client ? client.stop() : undefined;
}

module.exports = { activate, deactivate };
