// Hone Language Extension for VS Code
//
// This extension provides Language Server Protocol support for the Hone
// configuration language. It launches the `hone lsp` command and communicates
// with it via stdio.

import * as path from 'path';
import { workspace, ExtensionContext, window } from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

export function activate(context: ExtensionContext) {
    // Get the path to the Hone binary from settings
    const config = workspace.getConfiguration('hone');
    const serverPath = config.get<string>('serverPath', 'hone');

    // Server options - launch `hone lsp` via stdio
    const serverOptions: ServerOptions = {
        command: serverPath,
        args: ['lsp', '--stdio'],
        transport: TransportKind.stdio,
    };

    // Client options
    const clientOptions: LanguageClientOptions = {
        // Register for Hone files
        documentSelector: [{ scheme: 'file', language: 'hone' }],
        synchronize: {
            // Notify server about file changes to .hone files in the workspace
            fileEvents: workspace.createFileSystemWatcher('**/*.hone'),
        },
    };

    // Create and start the language client
    client = new LanguageClient(
        'hone',
        'Hone Language Server',
        serverOptions,
        clientOptions
    );

    // Start the client, which also launches the server
    client.start().catch((err) => {
        window.showErrorMessage(
            `Failed to start Hone language server: ${err.message}\n\n` +
            `Make sure 'hone' is installed and available in your PATH, ` +
            `or configure hone.serverPath in settings.`
        );
    });

    context.subscriptions.push({
        dispose: () => {
            if (client) {
                client.stop();
            }
        },
    });
}

export function deactivate(): Thenable<void> | undefined {
    if (client) {
        return client.stop();
    }
    return undefined;
}
