# Ferry user guide

How to use each part of Ferry. For building and installing, see the [README](../README.md#run-it).

## Sending a request

The window is laid out like Postman: a sidebar on the left, request tabs across the top, the request builder above the response.

1. Pick a method (GET, POST, …) or type your own, such as `PROPFIND`.
2. Type a URL, for example `https://httpbin.org/json`. Query parameters show up in the **Params** table, and editing the table updates the URL.
3. Add what the request needs:
   - **Authorization**: Basic Auth, Bearer Token or API Key.
   - **Headers**: your own rows, plus the headers Ferry adds (click "hidden" to see them).
   - **Body**: form-data (text and files), x-www-form-urlencoded, raw (Text, JSON with validation and Beautify, XML, HTML, JavaScript) or a binary file.
   - **Settings**: turn off following redirects or certificate verification.
4. Press **Send**, **Enter** in the URL field, or **Ctrl+Enter** (**⌘+Enter** on macOS).
5. Read the status, time (hover for the DNS / connect / waiting / download breakdown) and size. JSON is pretty-printed; switch to **Raw** or **Hex** at any time.

Tables have **Bulk Edit** for pasting `key: value` lines. Open tabs, including unsent requests, are restored when you start Ferry again.

## Collections, environments and variables

Ferry keeps your work in a **workspace folder** (by default `workspace` in Ferry's data directory; click the workspace name at the top left to pick another folder). Press **Ctrl+S** to save a request into a collection. Collections, folders and requests appear in the sidebar, where you can rename, duplicate, delete and drag them; every change is a plain file you can commit.

Write `{{name}}` anywhere in a request to use a variable. Values come from the environment picked in the top right, then the collection, then **Globals**. Mark a variable **Secret** to keep its value in a `*.local.toml` file next to the environment, which the workspace's `.gitignore` excludes. Dynamic variables such as `{{$guid}}`, `{{$timestamp}}` and `{{$randomInt}}` get a fresh value on every send.

## After you send

- **History** in the sidebar lists the last 100 requests with their responses; click one to open it again.
- **Cookies** set by responses are kept and sent with later requests to the same site. The response's **Cookies** tab lists what it set; **Cookies** in the footer manages the jar. Turn the jar off for one request in its **Settings**.
- **Console** in the footer logs every request exactly as sent, with its headers and body.
- **Search** (Ctrl+F) finds text in the response body; **Enter** and **Shift+Enter** step through matches.
- For JSON, type a **JSONPath** expression such as `$.items[*].name` above the body to see just those values.
- **Preview** shows HTML, images, audio, video and PDFs (Windows and macOS); **Open in Browser** opens the body with your default application.
- **Save as Example** keeps a response with its saved request.

## Scripts and tests

Each request, folder and collection has a **Scripts** tab with a **Pre-request** script, which runs before the request is sent, and a **Post-response** script, which runs after the response arrives. Scripts are JavaScript with Postman's `pm` API:

```js
// Pre-request: log in and keep the token for this environment.
pm.sendRequest({ url: pm.variables.get("base") + "/login", method: "POST" }, (err, res) => {
  pm.environment.set("token", res.json().token);
});

// Post-response: tests show up in the response's Test Results tab.
pm.test("status is 200", () => pm.response.to.have.status(200));
pm.test("has an id", () => pm.expect(pm.response.json().id).to.be.a("number"));
```

Scripts run in a sandbox with no file, process or network access (apart from `pm.sendRequest`), stop after 5 seconds or 64 MB, and log `console.log` output to the Console. Values scripts store with `pm.environment.set`, `pm.globals.set` or `pm.collectionVariables.set` go to the git-ignored `*.local.toml` files, so running scripts never changes the files you share.

`pm.visualizer.set(template, data)` adds a **Visualize** view to the response: a [Handlebars](https://handlebarsjs.com/) template rendered with your data. Its own scripts can run, for example to draw a chart, but it cannot load or send anything over the network.

## Code snippets and cURL

Click **</>** beside **Save** to open the **Code** panel: the current request as cURL, HTTPie, Python (requests), JavaScript (fetch), Node.js (axios), Go, Java (HttpClient), C# (HttpClient), PHP (cURL), Ruby (Net::HTTP), Rust (reqwest) or Swift (URLSession). Variables are filled in as they would be for sending. **Copy** puts the code on the clipboard, and the language you pick is remembered.

Paste a `curl` command into the URL field to turn it into a request: method, URL, headers, body, form fields, Basic auth, cookies and the `-L` / `-k` options all come across, and the request's scripts are kept. Commands copied from a browser's developer tools work in both the bash and Windows `cmd` styles. If the command can't be read, the reason shows under the URL and the request is left as it was.

## Collection Runner and CLI

**Run** on a collection or folder (its `…` menu, its settings, or **Runner** in the footer) opens the Collection Runner: pick the requests, the number of iterations, a delay and an optional CSV or JSON data file (one iteration per row, its columns available as variables and `pm.iterationData`), then **Run**. Results appear as they finish; **Export Results** saves a JSON report. `pm.execution.setNextRequest("Name")` jumps to another request.

The same runs work headless, for CI, with `ferry-cli`, which release archives include:

```sh
ferry-cli run collections/api --workspace path/to/workspace --environment staging --data users.csv --report report.json
```

It prints each request and its failed tests, and exits with 0 when everything passed, 1 when a request or test failed, and 2 for invalid arguments.

| Shortcut | Action |
|---|---|
| Ctrl+Enter | Send |
| Ctrl+T / Ctrl+W | New tab / close tab |
| Ctrl+Tab / Ctrl+Shift+Tab | Next / previous tab |
| Ctrl+B | Show or hide the sidebar |
| Ctrl+S | Save the request |
| Ctrl+F | Search the response body |

On macOS, use ⌘ instead of Ctrl (Ctrl+Tab stays Ctrl).

