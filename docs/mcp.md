# Connecting Pondus to Claude Desktop, ChatGPT and other agents (MCP)

Pondus is built for agents to use, and every operation of the program, from `add_frame` and `add_walls` to `analyze`, `design`, `show_picture` and `write_report`, is a tool of its Model Context Protocol (MCP) server. The server starts with `pondus --mcp` and speaks MCP over standard input and output (stdio, protocol version 2025-06-18). It currently offers 45 tools, and its instructions tell the agent how to model, check and report. An agent connected to it works on one structural model, and with `--autosave` every change is written to a `.pondus` file that the engineer then opens in the Pondus app to check the work.

```
pondus --mcp [--open model.pondus] [--autosave model.pondus]
```

- `--open` starts the server with an existing model (a `.pondus` file, or a Stabileo or Aedifex JSON model).
- `--autosave` saves the model after every change. Relative paths that the agent gives to `write_report`, `export_file` or `export_bim` are written next to this file, and the pictures of `show_picture` go to a `.pictures` folder beside it.

The path to the program depends on where Pondus was installed:

| Platform | Program |
|---|---|
| macOS (release app) | `/Applications/Pondus.app/Contents/MacOS/pondus` |
| Windows (release zip) | the folder you unzipped to, for example `C:\Pondus\pondus.exe` |
| Built from source | `target/release/pondus` in the repository |

## Claude Desktop

Claude Desktop starts local MCP servers itself over stdio, so Pondus connects to it directly.

1. Install Pondus from the [releases page](https://github.com/Sompote/pondus/releases) and make a folder for the model, for example `~/Pondus` (macOS) or `C:\Pondus` (Windows).
2. In Claude Desktop open Settings › Developer › Edit Config. This opens `claude_desktop_config.json`, which lives in `~/Library/Application Support/Claude/` on macOS and in `%APPDATA%\Claude\` on Windows.
3. Add Pondus under `mcpServers`. On macOS:

   ```json
   {
     "mcpServers": {
       "pondus": {
         "command": "/Applications/Pondus.app/Contents/MacOS/pondus",
         "args": ["--mcp", "--autosave", "/Users/you/Pondus/model.pondus"]
       }
     }
   }
   ```

   On Windows, write the backslashes of the paths twice:

   ```json
   {
     "mcpServers": {
       "pondus": {
         "command": "C:\\Pondus\\pondus.exe",
         "args": ["--mcp", "--autosave", "C:\\Pondus\\model.pondus"]
       }
     }
   }
   ```

   If the file already has other servers, add the `"pondus"` entry beside them inside the same `mcpServers` object.
4. Quit Claude Desktop completely and start it again. Pondus's tools then appear under the tools (search and tools) button of the message box, and Settings › Developer shows the server as running. If it shows an error, check the path to the program first.
5. Ask for a structure, for example: *"Use Pondus to build a 5-storey concrete frame, 3 bays of 6 m by 2 bays of 5 m, with a 6 m shear wall on grid line A, analyse it under ASCE 7 earthquake with SDS 0.5 and SD1 0.3, design it, and write the calculation report."* Claude calls the Pondus tools, shows the pictures that `show_picture` returns, and writes the report next to `model.pondus`.
6. Open `model.pondus` in the Pondus app (File › Open) to check the model, the diagrams and the design tables, or to carry on by hand.

**Claude Code.** The same server is added to Claude Code with one command:

```sh
claude mcp add pondus -- /Applications/Pondus.app/Contents/MacOS/pondus --mcp --autosave ~/Pondus/model.pondus
```

## ChatGPT

ChatGPT connects only to remote MCP servers that it reaches over HTTPS, and it cannot start a program on your computer. Pondus's stdio server therefore needs two helpers in front of it: a bridge that serves it over HTTP, and a tunnel that gives that HTTP server a public HTTPS address. The steps below use [supergateway](https://github.com/supercorp-ai/supergateway) as the bridge and a [Cloudflare quick tunnel](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/do-more-with-tunnels/trycloudflare/) as the tunnel. Any equivalent tools work, such as `mcp-proxy` or ngrok. These are third-party tools whose options change, so check their documentation if a command differs.

1. Serve Pondus over HTTP on your computer (Node.js needed for `npx`):

   ```sh
   npx -y supergateway \
     --stdio "/Applications/Pondus.app/Contents/MacOS/pondus --mcp --autosave $HOME/Pondus/model.pondus" \
     --outputTransport streamableHttp --port 8000
   ```

   The server then answers at `http://localhost:8000/mcp`.
2. In a second terminal, give it a public HTTPS address:

   ```sh
   cloudflared tunnel --url http://localhost:8000
   ```

   cloudflared prints an address such as `https://example-words.trycloudflare.com`. The MCP endpoint is that address followed by `/mcp`.
3. In ChatGPT, turn on developer mode for connectors (in recent versions Settings › Apps & Connectors › Advanced settings › Developer mode; the menu names have changed between versions and the feature depends on the plan). Create a new connector, give it the name Pondus and the endpoint URL from step 2, choose no authentication, and save it.
4. In a new chat, enable the Pondus connector (developer mode) and ask for the structure as above. ChatGPT asks you to confirm actions that change the model.
5. Open `~/Pondus/model.pondus` in the Pondus app to check the work.

**Security.** While the tunnel runs, anyone who has the URL can drive Pondus on your computer and read or overwrite the model and the files written next to it. Use a fresh folder for the model, keep the URL private, and stop the tunnel (Ctrl-C) when you are done. For lasting use, run the bridge on a server with authentication rather than through a quick tunnel.

**Licence.** If you let other people use Pondus over a network in this way, AGPL-3.0 section 13 asks you to offer them the source code of the version they use. The server already announces its licence and source link in its `serverInfo`, and its instructions tell the agent to give the link when asked. If you have modified Pondus, publish your modified source and point to it.

## Other agents

Any MCP client that can start a stdio server, such as the agents of Cursor, VS Code, Zed, LM Studio, TigrimOS or Aedifex, uses the same command, `pondus --mcp`, with the arguments above. Clients that only speak HTTP use the bridge described for ChatGPT, without the tunnel when they run on the same computer (`http://localhost:8000/mcp`).

## Checking the connection

Pondus answers a plain MCP handshake from a terminal, which is a quick test that the path and the program are right:

```sh
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}' \
  | /Applications/Pondus.app/Contents/MacOS/pondus --mcp
```

The reply names the server (`"name":"pondus"`) with its version, licence and source link.

Structural results produced through an agent are produced by software and must be checked by a qualified engineer.
