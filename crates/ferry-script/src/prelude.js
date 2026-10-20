// The `pm` API (F15-AC5..AC13) over the native bridge `__ferry`, which this
// prelude hides from scripts. Inputs arrive as JSON in `__ferry_input`;
// `__ferry_output()` returns what the script did as JSON.
(function (global) {
  "use strict";
  const bridge = global.__ferry;
  const input = JSON.parse(global.__ferry_input);
  delete global.__ferry;
  delete global.__ferry_input;

  const tests = [];
  let visualizer = null;
  let nextRequest; // undefined: not called; null: stop; string: name

  // --- formatting ------------------------------------------------------------------

  function typeOf(v) {
    if (v === null) return "null";
    if (Array.isArray(v)) return "array";
    if (v instanceof RegExp) return "regexp";
    if (v instanceof Date) return "date";
    if (v instanceof Error) return "error";
    return typeof v;
  }

  // Chai's way of showing a value in a message.
  function inspect(v, depth) {
    depth = depth || 0;
    switch (typeOf(v)) {
      case "string":
        return "'" + v.replace(/'/g, "\\'") + "'";
      case "number":
        return Object.is(v, -0) ? "-0" : String(v);
      case "bigint":
        return v + "n";
      case "undefined":
      case "null":
      case "boolean":
      case "symbol":
      case "regexp":
        return String(v);
      case "date":
        return isNaN(v) ? "Invalid Date" : v.toISOString();
      case "error":
        return "[" + v.name + ": " + v.message + "]";
      case "function":
        return "[Function" + (v.name ? " " + v.name : "") + "]";
      case "array":
        if (depth > 2) return "[Array]";
        return v.length === 0 ? "[]" : "[ " + v.map((x) => inspect(x, depth + 1)).join(", ") + " ]";
      default: {
        if (depth > 2) return "[Object]";
        const keys = Object.keys(v);
        if (keys.length === 0) return "{}";
        const key = (k) => (/^[A-Za-z_$][\w$]*$/.test(k) ? k : inspect(k));
        return "{ " + keys.map((k) => key(k) + ": " + inspect(v[k], depth + 1)).join(", ") + " }";
      }
    }
  }

  // How console arguments and variable values become text.
  function text(v) {
    if (typeof v === "string") return v;
    if (v instanceof Error) return v.toString();
    try {
      const json = JSON.stringify(v);
      return json === undefined ? String(v) : json;
    } catch (e) {
      return String(v);
    }
  }

  function deepEqual(a, b) {
    if (Object.is(a, b) || a === b) return true;
    const ta = typeOf(a);
    if (ta !== typeOf(b)) return false;
    if (ta === "date") return a.getTime() === b.getTime();
    if (ta === "regexp") return String(a) === String(b);
    if (ta === "array") {
      return a.length === b.length && a.every((x, i) => deepEqual(x, b[i]));
    }
    if (ta === "object") {
      const ka = Object.keys(a);
      const kb = Object.keys(b);
      return ka.length === kb.length && ka.every((k) => Object.prototype.hasOwnProperty.call(b, k) && deepEqual(a[k], b[k]));
    }
    return false;
  }

  // --- pm.expect (Chai BDD) ------------------------------------------------------------

  class AssertionError extends Error {
    constructor(message) {
      super(message);
      this.name = "AssertionError";
    }
  }

  function Assertion(obj, message) {
    this._obj = obj;
    this._message = message;
    this._not = false;
    this._deep = false;
  }

  function define(name, get) {
    Object.defineProperty(Assertion.prototype, name, { get, configurable: true });
  }

  for (const word of ["to", "be", "been", "is", "that", "which", "and", "has", "have", "with", "at", "of", "same", "but", "does", "still", "also"]) {
    define(word, function () {
      return this;
    });
  }
  define("not", function () {
    this._not = true;
    return this;
  });
  define("deep", function () {
    this._deep = true;
    return this;
  });

  Assertion.prototype._assert = function (ok, message, negated) {
    if (this._not ? ok : !ok) {
      const m = this._not ? negated : message;
      throw new AssertionError(this._message ? this._message + ": " + m : m);
    }
    return this;
  };

  Assertion.prototype._self = function () {
    return inspect(this._obj);
  };

  function property(name, check, message, negated) {
    define(name, function () {
      return this._assert(check(this._obj), "expected " + this._self() + " " + message, "expected " + this._self() + " " + negated);
    });
  }

  function isEmpty(v) {
    if (typeof v === "string" || Array.isArray(v)) return v.length === 0;
    if (v instanceof Map || v instanceof Set) return v.size === 0;
    if (v !== null && typeof v === "object") return Object.keys(v).length === 0;
    throw new AssertionError(".empty was passed non-string primitive " + inspect(v));
  }

  property("ok", (v) => !!v, "to be truthy", "to be falsy");
  property("true", (v) => v === true, "to be true", "to be false");
  property("false", (v) => v === false, "to be false", "to be true");
  property("null", (v) => v === null, "to be null", "not to be null");
  property("undefined", (v) => v === undefined, "to be undefined", "not to be undefined");
  property("NaN", (v) => typeof v === "number" && isNaN(v), "to be NaN", "not to be NaN");
  property("exist", (v) => v !== null && v !== undefined, "to exist", "to not exist");
  property("empty", isEmpty, "to be empty", "not to be empty");

  function method(names, fn) {
    for (const name of names) Assertion.prototype[name] = fn;
  }

  method(["equal", "equals", "eq"], function (expected) {
    if (this._deep) return this.eql(expected);
    return this._assert(this._obj === expected, "expected " + this._self() + " to equal " + inspect(expected), "expected " + this._self() + " to not equal " + inspect(expected));
  });
  method(["eql", "eqls"], function (expected) {
    return this._assert(deepEqual(this._obj, expected), "expected " + this._self() + " to deeply equal " + inspect(expected), "expected " + this._self() + " to not deeply equal " + inspect(expected));
  });

  function compare(names, test, word, negatedWord) {
    method(names, function (n) {
      return this._assert(test(this._obj, n), "expected " + this._self() + " to be " + word + " " + inspect(n), "expected " + this._self() + " to be " + negatedWord + " " + inspect(n));
    });
  }
  compare(["above", "gt", "greaterThan"], (a, n) => a > n, "above", "at most");
  compare(["below", "lt", "lessThan"], (a, n) => a < n, "below", "at least");
  compare(["least", "gte"], (a, n) => a >= n, "at least", "below");
  compare(["most", "lte"], (a, n) => a <= n, "at most", "above");

  method(["within"], function (low, high) {
    const range = inspect(low) + ".." + inspect(high);
    return this._assert(this._obj >= low && this._obj <= high, "expected " + this._self() + " to be within " + range, "expected " + this._self() + " to not be within " + range);
  });

  method(["a", "an"], function (type) {
    const want = String(type).toLowerCase();
    const article = /^[aeiou]/.test(want) ? "an " : "a ";
    return this._assert(typeOf(this._obj) === want, "expected " + this._self() + " to be " + article + want, "expected " + this._self() + " not to be " + article + want);
  });

  function includes(obj, value, deep) {
    const same = deep ? deepEqual : (a, b) => a === b || (Number.isNaN(a) && Number.isNaN(b));
    if (typeof obj === "string") return obj.indexOf(value) !== -1;
    if (Array.isArray(obj)) return obj.some((x) => same(x, value));
    if (obj instanceof Set) return [...obj].some((x) => same(x, value));
    if (obj !== null && typeof obj === "object" && value !== null && typeof value === "object") {
      return Object.keys(value).every((k) => k in obj && same(obj[k], value[k]));
    }
    return false;
  }
  method(["include", "includes", "contain", "contains"], function (value) {
    const word = this._deep ? "deep include " : "include ";
    return this._assert(includes(this._obj, value, this._deep), "expected " + this._self() + " to " + word + inspect(value), "expected " + this._self() + " to not " + word + inspect(value));
  });

  method(["lengthOf", "length"], function (n) {
    const len = this._obj === null || this._obj === undefined ? undefined : this._obj instanceof Map || this._obj instanceof Set ? this._obj.size : this._obj.length;
    return this._assert(len === n, "expected " + this._self() + " to have a length of " + inspect(n) + " but got " + inspect(len), "expected " + this._self() + " to not have a length of " + inspect(n));
  });

  method(["property"], function (name) {
    const obj = this._obj;
    const has = obj !== null && obj !== undefined && name in Object(obj);
    const actual = has ? obj[name] : undefined;
    if (arguments.length > 1) {
      const expected = arguments[1];
      const same = this._deep ? deepEqual(actual, expected) : actual === expected;
      this._assert(has && same, "expected " + this._self() + " to have " + (this._deep ? "deep " : "") + "property " + inspect(name) + " of " + inspect(expected) + ", but got " + inspect(actual), "expected " + this._self() + " to not have " + (this._deep ? "deep " : "") + "property " + inspect(name) + " of " + inspect(expected));
    } else {
      this._assert(has, "expected " + this._self() + " to have property " + inspect(name), "expected " + this._self() + " to not have property " + inspect(name));
    }
    if (!this._not) this._obj = actual;
    return this;
  });

  method(["match", "matches"], function (re) {
    return this._assert(re.test(String(this._obj)), "expected " + this._self() + " to match " + String(re), "expected " + this._self() + " not to match " + String(re));
  });

  method(["oneOf"], function (list) {
    const found = list.some((x) => (this._deep ? deepEqual(x, this._obj) : x === this._obj));
    return this._assert(found, "expected " + this._self() + " to be one of " + inspect(list), "expected " + this._self() + " to not be one of " + inspect(list));
  });

  method(["keys", "key"], function () {
    let wanted = Array.prototype.slice.call(arguments);
    if (wanted.length === 1 && Array.isArray(wanted[0])) wanted = wanted[0];
    if (wanted.length === 1 && wanted[0] !== null && typeof wanted[0] === "object") wanted = Object.keys(wanted[0]);
    const actual = this._obj === null || this._obj === undefined ? [] : Object.keys(this._obj);
    const ok = wanted.length > 0 && wanted.length === actual.length && wanted.every((k) => actual.indexOf(String(k)) !== -1);
    const shown = wanted.map((k) => inspect(String(k)));
    const list = shown.length > 1 ? shown.slice(0, -1).join(", ") + ", and " + shown[shown.length - 1] : shown.join("");
    const noun = shown.length > 1 ? "keys " : "key ";
    return this._assert(ok, "expected " + this._self() + " to have " + noun + list, "expected " + this._self() + " to not have " + noun + list);
  });

  // --- headers, URLs, requests and responses --------------------------------------------

  class HeaderList {
    constructor(pairs, readOnly) {
      this._list = pairs.map((p) => ({ key: p[0], value: p[1] }));
      this._readOnly = readOnly;
    }
    _find(key) {
      const lower = String(key).toLowerCase();
      return this._list.findIndex((h) => h.key.toLowerCase() === lower);
    }
    get(key) {
      const i = this._find(key);
      return i === -1 ? undefined : this._list[i].value;
    }
    has(key, value) {
      const i = this._find(key);
      return i !== -1 && (value === undefined || this._list[i].value === String(value));
    }
    all() {
      return this._list.map((h) => ({ key: h.key, value: h.value }));
    }
    each(fn) {
      this.all().forEach(fn);
    }
    count() {
      return this._list.length;
    }
    toObject() {
      const out = {};
      for (const h of this._list) out[h.key] = h.value;
      return out;
    }
    _change() {
      if (this._readOnly) throw new TypeError("These headers cannot be changed here");
    }
    add(header) {
      this._change();
      this._list.push({ key: String(header.key), value: String(header.value) });
    }
    upsert(header) {
      this._change();
      const i = this._find(header.key);
      if (i === -1) this.add(header);
      else this._list[i].value = String(header.value);
    }
    remove(key) {
      this._change();
      const lower = String(key).toLowerCase();
      this._list = this._list.filter((h) => h.key.toLowerCase() !== lower);
    }
    _pairs() {
      return this._list.map((h) => [h.key, h.value]);
    }
  }

  class Url {
    constructor(text) {
      this._text = String(text);
    }
    toString() {
      return this._text;
    }
    toJSON() {
      return this._text;
    }
    update(text) {
      this._text = String(text);
    }
    getHost() {
      const m = /^[a-z][a-z0-9+.-]*:\/\/(?:[^@/?#]*@)?([^/?#:]*)/i.exec(this._text);
      return m ? m[1] : "";
    }
    getPath() {
      const m = /^[a-z][a-z0-9+.-]*:\/\/[^/?#]*([^?#]*)/i.exec(this._text);
      return m && m[1] ? m[1] : "/";
    }
  }

  function makeRequest(r, readOnly) {
    let url = new Url(r.url);
    const body = { mode: r.body.mode, raw: r.body.raw };
    body.update = function (raw) {
      body.mode = "raw";
      body.raw = String(raw);
    };
    const request = {
      method: r.method,
      headers: new HeaderList(r.headers, readOnly),
      body,
    };
    Object.defineProperty(request, "url", {
      enumerable: true,
      get: () => url,
      set: (v) => {
        url = v instanceof Url ? v : new Url(v);
      },
    });
    return request;
  }

  function exportRequest(request) {
    return {
      method: String(request.method),
      url: String(request.url),
      headers: request.headers._pairs(),
      body: { mode: String(request.body.mode), raw: request.body.raw == null ? "" : String(request.body.raw) },
    };
  }

  const ranges = {
    ok: [(c) => c === 200, "200"],
    success: [(c) => c >= 200 && c < 300, "2XX"],
    redirection: [(c) => c >= 300 && c < 400, "3XX"],
    clientError: [(c) => c >= 400 && c < 500, "4XX"],
    serverError: [(c) => c >= 500 && c < 600, "5XX"],
    error: [(c) => c >= 400 && c < 600, "4XX or 5XX"],
    notFound: [(c) => c === 404, "404"],
  };

  function responseAssertions(response, not) {
    const check = (ok, message, negated) => {
      if (not ? ok : !ok) throw new AssertionError(not ? negated : message);
    };
    const have = {
      status(expected) {
        if (typeof expected === "number") {
          check(response.code === expected, "expected response to have status code " + expected + " but got " + response.code, "expected response to not have status code " + expected);
        } else {
          check(response.status === String(expected), "expected response to have status reason " + inspect(String(expected)) + " but got " + inspect(response.status), "expected response to not have status reason " + inspect(String(expected)));
        }
      },
      header(name, value) {
        const actual = response.headers.get(name);
        if (arguments.length < 2) {
          check(actual !== undefined, "expected response to have header with key " + inspect(String(name)), "expected response to not have header with key " + inspect(String(name)));
        } else {
          check(actual === String(value), "expected " + inspect(String(name)) + " response header to be " + inspect(String(value)) + " but got " + inspect(actual), "expected " + inspect(String(name)) + " response header to not be " + inspect(String(value)));
        }
      },
      body(expected) {
        const actual = response.text();
        check(actual === String(expected), "expected response body to equal " + inspect(String(expected)) + " but got " + inspect(actual), "expected response body to not equal " + inspect(String(expected)));
      },
      jsonBody() {
        let error = null;
        try {
          JSON.parse(response.text());
        } catch (e) {
          error = e;
        }
        check(error === null, "expected response body to be valid JSON but got error " + (error && error.message), "expected response body not to be valid JSON");
      },
    };
    const be = {};
    for (const name of Object.keys(ranges)) {
      Object.defineProperty(be, name, {
        enumerable: true,
        get() {
          const [test, label] = ranges[name];
          check(test(response.code), "expected response code to be " + label + " but found " + response.code, "expected response code to not be " + label + " but found " + response.code);
          return true;
        },
      });
    }
    const to = { have, be };
    if (!not) {
      Object.defineProperty(to, "not", { get: () => responseAssertions(response, true) });
    }
    return to;
  }

  function makeResponse(r) {
    const response = {
      code: r.code,
      status: r.status,
      headers: new HeaderList(r.headers, true),
      responseTime: r.responseTime,
      responseSize: r.responseSize,
      text() {
        return r.body;
      },
      json() {
        try {
          return JSON.parse(r.body);
        } catch (e) {
          throw new Error("Response body is not valid JSON: " + e.message);
        }
      },
    };
    Object.defineProperty(response, "to", { get: () => responseAssertions(response, false) });
    return response;
  }

  // --- variables -----------------------------------------------------------------------

  function scope(name, extra) {
    const s = {
      get: (key) => bridge.get(name, String(key)),
      has: (key) => bridge.has(name, String(key)),
      set: (key, value) => {
        bridge.set(name, String(key), text(value));
      },
      unset: (key) => {
        bridge.unset(name, String(key));
      },
      toObject: () => JSON.parse(bridge.all(name)),
    };
    return Object.assign(s, extra || {});
  }

  // --- pm ------------------------------------------------------------------------------

  const request = makeRequest(input.request, input.event === "test");
  const response = input.response ? makeResponse(input.response) : undefined;

  function test(name, fn) {
    const record = { name: String(name), status: "passed", error: null };
    try {
      if (typeof fn === "function") fn();
    } catch (e) {
      record.status = "failed";
      record.error = e && e.message !== undefined ? String(e.message) : text(e);
    }
    tests.push(record);
  }
  test.skip = function (name) {
    tests.push({ name: String(name), status: "skipped", error: null });
  };

  const iterationData = input.data;
  const execution = {
    setNextRequest(name) {
      nextRequest = name === null || name === undefined ? null : String(name);
    },
  };

  const pm = {
    info: Object.freeze({
      requestName: input.info.requestName,
      eventName: input.event,
      iteration: input.info.iteration,
      iterationCount: input.info.iterationCount,
    }),
    environment: scope("environment", { name: input.environmentName }),
    globals: scope("globals"),
    collectionVariables: scope("collection"),
    variables: scope("variables", {
      replaceIn: (t) => bridge.replaceIn(String(t)),
    }),
    iterationData: {
      get: (key) => (Object.prototype.hasOwnProperty.call(iterationData, key) ? iterationData[key] : undefined),
      has: (key) => Object.prototype.hasOwnProperty.call(iterationData, key),
      toObject: () => Object.assign({}, iterationData),
    },
    request,
    response,
    test,
    expect: (value, message) => new Assertion(value, message),
    sendRequest(req, callback) {
      const spec = typeof req === "string" ? { url: req } : req || {};
      const header = spec.header || spec.headers || [];
      const pairs = Array.isArray(header) ? header.map((h) => [String(h.key), String(h.value)]) : Object.keys(header).map((k) => [k, String(header[k])]);
      const body = spec.body && spec.body.mode === "raw" ? String(spec.body.raw == null ? "" : spec.body.raw) : "";
      const result = JSON.parse(bridge.send(JSON.stringify({ method: String(spec.method || "GET"), url: String(spec.url || ""), headers: pairs, body: { mode: body ? "raw" : "none", raw: body } })));
      if (typeof callback !== "function") return;
      if (result.error !== undefined) callback({ message: result.error }, null);
      else callback(null, makeResponse(result.response));
    },
    visualizer: {
      set(template, data) {
        visualizer = { template: String(template), data: JSON.stringify(data === undefined ? {} : data) };
      },
      clear() {
        visualizer = null;
      },
    },
    execution,
  };

  const log = (level) =>
    function () {
      bridge.console(level, Array.prototype.map.call(arguments, text).join(" "));
    };
  const console = { log: log("log"), info: log("info"), warn: log("warn"), error: log("error"), debug: log("log") };

  Object.defineProperty(global, "pm", { value: Object.freeze(pm), enumerable: true });
  Object.defineProperty(global, "postman", { value: Object.freeze({ setNextRequest: execution.setNextRequest }), enumerable: true });
  global.console = console;
  global.require = function () {
    throw new Error("require is not available in Ferry scripts");
  };

  Object.defineProperty(global, "__ferry_output", {
    value: function () {
      return JSON.stringify({
        request: exportRequest(request),
        tests,
        visualizer,
        nextRequest: nextRequest === undefined ? { set: false } : { set: true, name: nextRequest },
      });
    },
  });
})(globalThis);
