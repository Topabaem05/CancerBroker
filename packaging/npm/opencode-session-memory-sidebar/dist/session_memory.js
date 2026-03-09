// node_modules/@opencode-ai/sdk/dist/gen/core/serverSentEvents.gen.js
var createSseClient = ({ onSseError, onSseEvent, responseTransformer, responseValidator, sseDefaultRetryDelay, sseMaxRetryAttempts, sseMaxRetryDelay, sseSleepFn, url, ...options }) => {
  let lastEventId;
  const sleep = sseSleepFn ?? ((ms) => new Promise((resolve) => setTimeout(resolve, ms)));
  const createStream = async function* () {
    let retryDelay = sseDefaultRetryDelay ?? 3e3;
    let attempt = 0;
    const signal = options.signal ?? new AbortController().signal;
    while (true) {
      if (signal.aborted)
        break;
      attempt++;
      const headers = options.headers instanceof Headers ? options.headers : new Headers(options.headers);
      if (lastEventId !== void 0) {
        headers.set("Last-Event-ID", lastEventId);
      }
      try {
        const response = await fetch(url, { ...options, headers, signal });
        if (!response.ok)
          throw new Error(`SSE failed: ${response.status} ${response.statusText}`);
        if (!response.body)
          throw new Error("No body in SSE response");
        const reader = response.body.pipeThrough(new TextDecoderStream()).getReader();
        let buffer = "";
        const abortHandler = () => {
          try {
            reader.cancel();
          } catch {
          }
        };
        signal.addEventListener("abort", abortHandler);
        try {
          while (true) {
            const { done, value } = await reader.read();
            if (done)
              break;
            buffer += value;
            const chunks = buffer.split("\n\n");
            buffer = chunks.pop() ?? "";
            for (const chunk of chunks) {
              const lines = chunk.split("\n");
              const dataLines = [];
              let eventName;
              for (const line of lines) {
                if (line.startsWith("data:")) {
                  dataLines.push(line.replace(/^data:\s*/, ""));
                } else if (line.startsWith("event:")) {
                  eventName = line.replace(/^event:\s*/, "");
                } else if (line.startsWith("id:")) {
                  lastEventId = line.replace(/^id:\s*/, "");
                } else if (line.startsWith("retry:")) {
                  const parsed = Number.parseInt(line.replace(/^retry:\s*/, ""), 10);
                  if (!Number.isNaN(parsed)) {
                    retryDelay = parsed;
                  }
                }
              }
              let data;
              let parsedJson = false;
              if (dataLines.length) {
                const rawData = dataLines.join("\n");
                try {
                  data = JSON.parse(rawData);
                  parsedJson = true;
                } catch {
                  data = rawData;
                }
              }
              if (parsedJson) {
                if (responseValidator) {
                  await responseValidator(data);
                }
                if (responseTransformer) {
                  data = await responseTransformer(data);
                }
              }
              onSseEvent?.({
                data,
                event: eventName,
                id: lastEventId,
                retry: retryDelay
              });
              if (dataLines.length) {
                yield data;
              }
            }
          }
        } finally {
          signal.removeEventListener("abort", abortHandler);
          reader.releaseLock();
        }
        break;
      } catch (error) {
        onSseError?.(error);
        if (sseMaxRetryAttempts !== void 0 && attempt >= sseMaxRetryAttempts) {
          break;
        }
        const backoff = Math.min(retryDelay * 2 ** (attempt - 1), sseMaxRetryDelay ?? 3e4);
        await sleep(backoff);
      }
    }
  };
  const stream = createStream();
  return { stream };
};

// node_modules/@opencode-ai/sdk/dist/gen/core/auth.gen.js
var getAuthToken = async (auth, callback) => {
  const token = typeof callback === "function" ? await callback(auth) : callback;
  if (!token) {
    return;
  }
  if (auth.scheme === "bearer") {
    return `Bearer ${token}`;
  }
  if (auth.scheme === "basic") {
    return `Basic ${btoa(token)}`;
  }
  return token;
};

// node_modules/@opencode-ai/sdk/dist/gen/core/bodySerializer.gen.js
var jsonBodySerializer = {
  bodySerializer: (body) => JSON.stringify(body, (_key, value) => typeof value === "bigint" ? value.toString() : value)
};

// node_modules/@opencode-ai/sdk/dist/gen/core/pathSerializer.gen.js
var separatorArrayExplode = (style) => {
  switch (style) {
    case "label":
      return ".";
    case "matrix":
      return ";";
    case "simple":
      return ",";
    default:
      return "&";
  }
};
var separatorArrayNoExplode = (style) => {
  switch (style) {
    case "form":
      return ",";
    case "pipeDelimited":
      return "|";
    case "spaceDelimited":
      return "%20";
    default:
      return ",";
  }
};
var separatorObjectExplode = (style) => {
  switch (style) {
    case "label":
      return ".";
    case "matrix":
      return ";";
    case "simple":
      return ",";
    default:
      return "&";
  }
};
var serializeArrayParam = ({ allowReserved, explode, name, style, value }) => {
  if (!explode) {
    const joinedValues2 = (allowReserved ? value : value.map((v) => encodeURIComponent(v))).join(separatorArrayNoExplode(style));
    switch (style) {
      case "label":
        return `.${joinedValues2}`;
      case "matrix":
        return `;${name}=${joinedValues2}`;
      case "simple":
        return joinedValues2;
      default:
        return `${name}=${joinedValues2}`;
    }
  }
  const separator = separatorArrayExplode(style);
  const joinedValues = value.map((v) => {
    if (style === "label" || style === "simple") {
      return allowReserved ? v : encodeURIComponent(v);
    }
    return serializePrimitiveParam({
      allowReserved,
      name,
      value: v
    });
  }).join(separator);
  return style === "label" || style === "matrix" ? separator + joinedValues : joinedValues;
};
var serializePrimitiveParam = ({ allowReserved, name, value }) => {
  if (value === void 0 || value === null) {
    return "";
  }
  if (typeof value === "object") {
    throw new Error("Deeply-nested arrays/objects aren\u2019t supported. Provide your own `querySerializer()` to handle these.");
  }
  return `${name}=${allowReserved ? value : encodeURIComponent(value)}`;
};
var serializeObjectParam = ({ allowReserved, explode, name, style, value, valueOnly }) => {
  if (value instanceof Date) {
    return valueOnly ? value.toISOString() : `${name}=${value.toISOString()}`;
  }
  if (style !== "deepObject" && !explode) {
    let values = [];
    Object.entries(value).forEach(([key, v]) => {
      values = [...values, key, allowReserved ? v : encodeURIComponent(v)];
    });
    const joinedValues2 = values.join(",");
    switch (style) {
      case "form":
        return `${name}=${joinedValues2}`;
      case "label":
        return `.${joinedValues2}`;
      case "matrix":
        return `;${name}=${joinedValues2}`;
      default:
        return joinedValues2;
    }
  }
  const separator = separatorObjectExplode(style);
  const joinedValues = Object.entries(value).map(([key, v]) => serializePrimitiveParam({
    allowReserved,
    name: style === "deepObject" ? `${name}[${key}]` : key,
    value: v
  })).join(separator);
  return style === "label" || style === "matrix" ? separator + joinedValues : joinedValues;
};

// node_modules/@opencode-ai/sdk/dist/gen/core/utils.gen.js
var PATH_PARAM_RE = /\{[^{}]+\}/g;
var defaultPathSerializer = ({ path, url: _url }) => {
  let url = _url;
  const matches = _url.match(PATH_PARAM_RE);
  if (matches) {
    for (const match of matches) {
      let explode = false;
      let name = match.substring(1, match.length - 1);
      let style = "simple";
      if (name.endsWith("*")) {
        explode = true;
        name = name.substring(0, name.length - 1);
      }
      if (name.startsWith(".")) {
        name = name.substring(1);
        style = "label";
      } else if (name.startsWith(";")) {
        name = name.substring(1);
        style = "matrix";
      }
      const value = path[name];
      if (value === void 0 || value === null) {
        continue;
      }
      if (Array.isArray(value)) {
        url = url.replace(match, serializeArrayParam({ explode, name, style, value }));
        continue;
      }
      if (typeof value === "object") {
        url = url.replace(match, serializeObjectParam({
          explode,
          name,
          style,
          value,
          valueOnly: true
        }));
        continue;
      }
      if (style === "matrix") {
        url = url.replace(match, `;${serializePrimitiveParam({
          name,
          value
        })}`);
        continue;
      }
      const replaceValue = encodeURIComponent(style === "label" ? `.${value}` : value);
      url = url.replace(match, replaceValue);
    }
  }
  return url;
};
var getUrl = ({ baseUrl, path, query, querySerializer, url: _url }) => {
  const pathUrl = _url.startsWith("/") ? _url : `/${_url}`;
  let url = (baseUrl ?? "") + pathUrl;
  if (path) {
    url = defaultPathSerializer({ path, url });
  }
  let search = query ? querySerializer(query) : "";
  if (search.startsWith("?")) {
    search = search.substring(1);
  }
  if (search) {
    url += `?${search}`;
  }
  return url;
};

// node_modules/@opencode-ai/sdk/dist/gen/client/utils.gen.js
var createQuerySerializer = ({ allowReserved, array, object } = {}) => {
  const querySerializer = (queryParams) => {
    const search = [];
    if (queryParams && typeof queryParams === "object") {
      for (const name in queryParams) {
        const value = queryParams[name];
        if (value === void 0 || value === null) {
          continue;
        }
        if (Array.isArray(value)) {
          const serializedArray = serializeArrayParam({
            allowReserved,
            explode: true,
            name,
            style: "form",
            value,
            ...array
          });
          if (serializedArray)
            search.push(serializedArray);
        } else if (typeof value === "object") {
          const serializedObject = serializeObjectParam({
            allowReserved,
            explode: true,
            name,
            style: "deepObject",
            value,
            ...object
          });
          if (serializedObject)
            search.push(serializedObject);
        } else {
          const serializedPrimitive = serializePrimitiveParam({
            allowReserved,
            name,
            value
          });
          if (serializedPrimitive)
            search.push(serializedPrimitive);
        }
      }
    }
    return search.join("&");
  };
  return querySerializer;
};
var getParseAs = (contentType) => {
  if (!contentType) {
    return "stream";
  }
  const cleanContent = contentType.split(";")[0]?.trim();
  if (!cleanContent) {
    return;
  }
  if (cleanContent.startsWith("application/json") || cleanContent.endsWith("+json")) {
    return "json";
  }
  if (cleanContent === "multipart/form-data") {
    return "formData";
  }
  if (["application/", "audio/", "image/", "video/"].some((type) => cleanContent.startsWith(type))) {
    return "blob";
  }
  if (cleanContent.startsWith("text/")) {
    return "text";
  }
  return;
};
var checkForExistence = (options, name) => {
  if (!name) {
    return false;
  }
  if (options.headers.has(name) || options.query?.[name] || options.headers.get("Cookie")?.includes(`${name}=`)) {
    return true;
  }
  return false;
};
var setAuthParams = async ({ security, ...options }) => {
  for (const auth of security) {
    if (checkForExistence(options, auth.name)) {
      continue;
    }
    const token = await getAuthToken(auth, options.auth);
    if (!token) {
      continue;
    }
    const name = auth.name ?? "Authorization";
    switch (auth.in) {
      case "query":
        if (!options.query) {
          options.query = {};
        }
        options.query[name] = token;
        break;
      case "cookie":
        options.headers.append("Cookie", `${name}=${token}`);
        break;
      case "header":
      default:
        options.headers.set(name, token);
        break;
    }
  }
};
var buildUrl = (options) => getUrl({
  baseUrl: options.baseUrl,
  path: options.path,
  query: options.query,
  querySerializer: typeof options.querySerializer === "function" ? options.querySerializer : createQuerySerializer(options.querySerializer),
  url: options.url
});
var mergeConfigs = (a, b) => {
  const config = { ...a, ...b };
  if (config.baseUrl?.endsWith("/")) {
    config.baseUrl = config.baseUrl.substring(0, config.baseUrl.length - 1);
  }
  config.headers = mergeHeaders(a.headers, b.headers);
  return config;
};
var mergeHeaders = (...headers) => {
  const mergedHeaders = new Headers();
  for (const header of headers) {
    if (!header || typeof header !== "object") {
      continue;
    }
    const iterator = header instanceof Headers ? header.entries() : Object.entries(header);
    for (const [key, value] of iterator) {
      if (value === null) {
        mergedHeaders.delete(key);
      } else if (Array.isArray(value)) {
        for (const v of value) {
          mergedHeaders.append(key, v);
        }
      } else if (value !== void 0) {
        mergedHeaders.set(key, typeof value === "object" ? JSON.stringify(value) : value);
      }
    }
  }
  return mergedHeaders;
};
var Interceptors = class {
  _fns;
  constructor() {
    this._fns = [];
  }
  clear() {
    this._fns = [];
  }
  getInterceptorIndex(id) {
    if (typeof id === "number") {
      return this._fns[id] ? id : -1;
    } else {
      return this._fns.indexOf(id);
    }
  }
  exists(id) {
    const index = this.getInterceptorIndex(id);
    return !!this._fns[index];
  }
  eject(id) {
    const index = this.getInterceptorIndex(id);
    if (this._fns[index]) {
      this._fns[index] = null;
    }
  }
  update(id, fn) {
    const index = this.getInterceptorIndex(id);
    if (this._fns[index]) {
      this._fns[index] = fn;
      return id;
    } else {
      return false;
    }
  }
  use(fn) {
    this._fns = [...this._fns, fn];
    return this._fns.length - 1;
  }
};
var createInterceptors = () => ({
  error: new Interceptors(),
  request: new Interceptors(),
  response: new Interceptors()
});
var defaultQuerySerializer = createQuerySerializer({
  allowReserved: false,
  array: {
    explode: true,
    style: "form"
  },
  object: {
    explode: true,
    style: "deepObject"
  }
});
var defaultHeaders = {
  "Content-Type": "application/json"
};
var createConfig = (override = {}) => ({
  ...jsonBodySerializer,
  headers: defaultHeaders,
  parseAs: "auto",
  querySerializer: defaultQuerySerializer,
  ...override
});

// node_modules/@opencode-ai/sdk/dist/gen/client/client.gen.js
var createClient = (config = {}) => {
  let _config = mergeConfigs(createConfig(), config);
  const getConfig = () => ({ ..._config });
  const setConfig = (config2) => {
    _config = mergeConfigs(_config, config2);
    return getConfig();
  };
  const interceptors = createInterceptors();
  const beforeRequest = async (options) => {
    const opts = {
      ..._config,
      ...options,
      fetch: options.fetch ?? _config.fetch ?? globalThis.fetch,
      headers: mergeHeaders(_config.headers, options.headers),
      serializedBody: void 0
    };
    if (opts.security) {
      await setAuthParams({
        ...opts,
        security: opts.security
      });
    }
    if (opts.requestValidator) {
      await opts.requestValidator(opts);
    }
    if (opts.body && opts.bodySerializer) {
      opts.serializedBody = opts.bodySerializer(opts.body);
    }
    if (opts.serializedBody === void 0 || opts.serializedBody === "") {
      opts.headers.delete("Content-Type");
    }
    const url = buildUrl(opts);
    return { opts, url };
  };
  const request = async (options) => {
    const { opts, url } = await beforeRequest(options);
    const requestInit = {
      redirect: "follow",
      ...opts,
      body: opts.serializedBody
    };
    let request2 = new Request(url, requestInit);
    for (const fn of interceptors.request._fns) {
      if (fn) {
        request2 = await fn(request2, opts);
      }
    }
    const _fetch = opts.fetch;
    let response = await _fetch(request2);
    for (const fn of interceptors.response._fns) {
      if (fn) {
        response = await fn(response, request2, opts);
      }
    }
    const result = {
      request: request2,
      response
    };
    if (response.ok) {
      if (response.status === 204 || response.headers.get("Content-Length") === "0") {
        return opts.responseStyle === "data" ? {} : {
          data: {},
          ...result
        };
      }
      const parseAs = (opts.parseAs === "auto" ? getParseAs(response.headers.get("Content-Type")) : opts.parseAs) ?? "json";
      let data;
      switch (parseAs) {
        case "arrayBuffer":
        case "blob":
        case "formData":
        case "json":
        case "text":
          data = await response[parseAs]();
          break;
        case "stream":
          return opts.responseStyle === "data" ? response.body : {
            data: response.body,
            ...result
          };
      }
      if (parseAs === "json") {
        if (opts.responseValidator) {
          await opts.responseValidator(data);
        }
        if (opts.responseTransformer) {
          data = await opts.responseTransformer(data);
        }
      }
      return opts.responseStyle === "data" ? data : {
        data,
        ...result
      };
    }
    const textError = await response.text();
    let jsonError;
    try {
      jsonError = JSON.parse(textError);
    } catch {
    }
    const error = jsonError ?? textError;
    let finalError = error;
    for (const fn of interceptors.error._fns) {
      if (fn) {
        finalError = await fn(error, response, request2, opts);
      }
    }
    finalError = finalError || {};
    if (opts.throwOnError) {
      throw finalError;
    }
    return opts.responseStyle === "data" ? void 0 : {
      error: finalError,
      ...result
    };
  };
  const makeMethod = (method) => {
    const fn = (options) => request({ ...options, method });
    fn.sse = async (options) => {
      const { opts, url } = await beforeRequest(options);
      return createSseClient({
        ...opts,
        body: opts.body,
        headers: opts.headers,
        method,
        url
      });
    };
    return fn;
  };
  return {
    buildUrl,
    connect: makeMethod("CONNECT"),
    delete: makeMethod("DELETE"),
    get: makeMethod("GET"),
    getConfig,
    head: makeMethod("HEAD"),
    interceptors,
    options: makeMethod("OPTIONS"),
    patch: makeMethod("PATCH"),
    post: makeMethod("POST"),
    put: makeMethod("PUT"),
    request,
    setConfig,
    trace: makeMethod("TRACE")
  };
};

// node_modules/@opencode-ai/sdk/dist/gen/core/params.gen.js
var extraPrefixesMap = {
  $body_: "body",
  $headers_: "headers",
  $path_: "path",
  $query_: "query"
};
var extraPrefixes = Object.entries(extraPrefixesMap);

// node_modules/@opencode-ai/sdk/dist/gen/client.gen.js
var client = createClient(createConfig({
  baseUrl: "http://localhost:4096"
}));

// node_modules/@opencode-ai/sdk/dist/gen/sdk.gen.js
var _HeyApiClient = class {
  _client = client;
  constructor(args) {
    if (args?.client) {
      this._client = args.client;
    }
  }
};
var Global = class extends _HeyApiClient {
  /**
   * Get events
   */
  event(options) {
    return (options?.client ?? this._client).get.sse({
      url: "/global/event",
      ...options
    });
  }
};
var Project = class extends _HeyApiClient {
  /**
   * List all projects
   */
  list(options) {
    return (options?.client ?? this._client).get({
      url: "/project",
      ...options
    });
  }
  /**
   * Get the current project
   */
  current(options) {
    return (options?.client ?? this._client).get({
      url: "/project/current",
      ...options
    });
  }
};
var Pty = class extends _HeyApiClient {
  /**
   * List all PTY sessions
   */
  list(options) {
    return (options?.client ?? this._client).get({
      url: "/pty",
      ...options
    });
  }
  /**
   * Create a new PTY session
   */
  create(options) {
    return (options?.client ?? this._client).post({
      url: "/pty",
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options?.headers
      }
    });
  }
  /**
   * Remove a PTY session
   */
  remove(options) {
    return (options.client ?? this._client).delete({
      url: "/pty/{id}",
      ...options
    });
  }
  /**
   * Get PTY session info
   */
  get(options) {
    return (options.client ?? this._client).get({
      url: "/pty/{id}",
      ...options
    });
  }
  /**
   * Update PTY session
   */
  update(options) {
    return (options.client ?? this._client).put({
      url: "/pty/{id}",
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options.headers
      }
    });
  }
  /**
   * Connect to a PTY session
   */
  connect(options) {
    return (options.client ?? this._client).get({
      url: "/pty/{id}/connect",
      ...options
    });
  }
};
var Config = class extends _HeyApiClient {
  /**
   * Get config info
   */
  get(options) {
    return (options?.client ?? this._client).get({
      url: "/config",
      ...options
    });
  }
  /**
   * Update config
   */
  update(options) {
    return (options?.client ?? this._client).patch({
      url: "/config",
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options?.headers
      }
    });
  }
  /**
   * List all providers
   */
  providers(options) {
    return (options?.client ?? this._client).get({
      url: "/config/providers",
      ...options
    });
  }
};
var Tool = class extends _HeyApiClient {
  /**
   * List all tool IDs (including built-in and dynamically registered)
   */
  ids(options) {
    return (options?.client ?? this._client).get({
      url: "/experimental/tool/ids",
      ...options
    });
  }
  /**
   * List tools with JSON schema parameters for a provider/model
   */
  list(options) {
    return (options.client ?? this._client).get({
      url: "/experimental/tool",
      ...options
    });
  }
};
var Instance = class extends _HeyApiClient {
  /**
   * Dispose the current instance
   */
  dispose(options) {
    return (options?.client ?? this._client).post({
      url: "/instance/dispose",
      ...options
    });
  }
};
var Path = class extends _HeyApiClient {
  /**
   * Get the current path
   */
  get(options) {
    return (options?.client ?? this._client).get({
      url: "/path",
      ...options
    });
  }
};
var Vcs = class extends _HeyApiClient {
  /**
   * Get VCS info for the current instance
   */
  get(options) {
    return (options?.client ?? this._client).get({
      url: "/vcs",
      ...options
    });
  }
};
var Session = class extends _HeyApiClient {
  /**
   * List all sessions
   */
  list(options) {
    return (options?.client ?? this._client).get({
      url: "/session",
      ...options
    });
  }
  /**
   * Create a new session
   */
  create(options) {
    return (options?.client ?? this._client).post({
      url: "/session",
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options?.headers
      }
    });
  }
  /**
   * Get session status
   */
  status(options) {
    return (options?.client ?? this._client).get({
      url: "/session/status",
      ...options
    });
  }
  /**
   * Delete a session and all its data
   */
  delete(options) {
    return (options.client ?? this._client).delete({
      url: "/session/{id}",
      ...options
    });
  }
  /**
   * Get session
   */
  get(options) {
    return (options.client ?? this._client).get({
      url: "/session/{id}",
      ...options
    });
  }
  /**
   * Update session properties
   */
  update(options) {
    return (options.client ?? this._client).patch({
      url: "/session/{id}",
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options.headers
      }
    });
  }
  /**
   * Get a session's children
   */
  children(options) {
    return (options.client ?? this._client).get({
      url: "/session/{id}/children",
      ...options
    });
  }
  /**
   * Get the todo list for a session
   */
  todo(options) {
    return (options.client ?? this._client).get({
      url: "/session/{id}/todo",
      ...options
    });
  }
  /**
   * Analyze the app and create an AGENTS.md file
   */
  init(options) {
    return (options.client ?? this._client).post({
      url: "/session/{id}/init",
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options.headers
      }
    });
  }
  /**
   * Fork an existing session at a specific message
   */
  fork(options) {
    return (options.client ?? this._client).post({
      url: "/session/{id}/fork",
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options.headers
      }
    });
  }
  /**
   * Abort a session
   */
  abort(options) {
    return (options.client ?? this._client).post({
      url: "/session/{id}/abort",
      ...options
    });
  }
  /**
   * Unshare the session
   */
  unshare(options) {
    return (options.client ?? this._client).delete({
      url: "/session/{id}/share",
      ...options
    });
  }
  /**
   * Share a session
   */
  share(options) {
    return (options.client ?? this._client).post({
      url: "/session/{id}/share",
      ...options
    });
  }
  /**
   * Get the diff for this session
   */
  diff(options) {
    return (options.client ?? this._client).get({
      url: "/session/{id}/diff",
      ...options
    });
  }
  /**
   * Summarize the session
   */
  summarize(options) {
    return (options.client ?? this._client).post({
      url: "/session/{id}/summarize",
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options.headers
      }
    });
  }
  /**
   * List messages for a session
   */
  messages(options) {
    return (options.client ?? this._client).get({
      url: "/session/{id}/message",
      ...options
    });
  }
  /**
   * Create and send a new message to a session
   */
  prompt(options) {
    return (options.client ?? this._client).post({
      url: "/session/{id}/message",
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options.headers
      }
    });
  }
  /**
   * Get a message from a session
   */
  message(options) {
    return (options.client ?? this._client).get({
      url: "/session/{id}/message/{messageID}",
      ...options
    });
  }
  /**
   * Create and send a new message to a session, start if needed and return immediately
   */
  promptAsync(options) {
    return (options.client ?? this._client).post({
      url: "/session/{id}/prompt_async",
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options.headers
      }
    });
  }
  /**
   * Send a new command to a session
   */
  command(options) {
    return (options.client ?? this._client).post({
      url: "/session/{id}/command",
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options.headers
      }
    });
  }
  /**
   * Run a shell command
   */
  shell(options) {
    return (options.client ?? this._client).post({
      url: "/session/{id}/shell",
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options.headers
      }
    });
  }
  /**
   * Revert a message
   */
  revert(options) {
    return (options.client ?? this._client).post({
      url: "/session/{id}/revert",
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options.headers
      }
    });
  }
  /**
   * Restore all reverted messages
   */
  unrevert(options) {
    return (options.client ?? this._client).post({
      url: "/session/{id}/unrevert",
      ...options
    });
  }
};
var Command = class extends _HeyApiClient {
  /**
   * List all commands
   */
  list(options) {
    return (options?.client ?? this._client).get({
      url: "/command",
      ...options
    });
  }
};
var Oauth = class extends _HeyApiClient {
  /**
   * Authorize a provider using OAuth
   */
  authorize(options) {
    return (options.client ?? this._client).post({
      url: "/provider/{id}/oauth/authorize",
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options.headers
      }
    });
  }
  /**
   * Handle OAuth callback for a provider
   */
  callback(options) {
    return (options.client ?? this._client).post({
      url: "/provider/{id}/oauth/callback",
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options.headers
      }
    });
  }
};
var Provider = class extends _HeyApiClient {
  /**
   * List all providers
   */
  list(options) {
    return (options?.client ?? this._client).get({
      url: "/provider",
      ...options
    });
  }
  /**
   * Get provider authentication methods
   */
  auth(options) {
    return (options?.client ?? this._client).get({
      url: "/provider/auth",
      ...options
    });
  }
  oauth = new Oauth({ client: this._client });
};
var Find = class extends _HeyApiClient {
  /**
   * Find text in files
   */
  text(options) {
    return (options.client ?? this._client).get({
      url: "/find",
      ...options
    });
  }
  /**
   * Find files
   */
  files(options) {
    return (options.client ?? this._client).get({
      url: "/find/file",
      ...options
    });
  }
  /**
   * Find workspace symbols
   */
  symbols(options) {
    return (options.client ?? this._client).get({
      url: "/find/symbol",
      ...options
    });
  }
};
var File = class extends _HeyApiClient {
  /**
   * List files and directories
   */
  list(options) {
    return (options.client ?? this._client).get({
      url: "/file",
      ...options
    });
  }
  /**
   * Read a file
   */
  read(options) {
    return (options.client ?? this._client).get({
      url: "/file/content",
      ...options
    });
  }
  /**
   * Get file status
   */
  status(options) {
    return (options?.client ?? this._client).get({
      url: "/file/status",
      ...options
    });
  }
};
var App = class extends _HeyApiClient {
  /**
   * Write a log entry to the server logs
   */
  log(options) {
    return (options?.client ?? this._client).post({
      url: "/log",
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options?.headers
      }
    });
  }
  /**
   * List all agents
   */
  agents(options) {
    return (options?.client ?? this._client).get({
      url: "/agent",
      ...options
    });
  }
};
var Auth = class extends _HeyApiClient {
  /**
   * Remove OAuth credentials for an MCP server
   */
  remove(options) {
    return (options.client ?? this._client).delete({
      url: "/mcp/{name}/auth",
      ...options
    });
  }
  /**
   * Start OAuth authentication flow for an MCP server
   */
  start(options) {
    return (options.client ?? this._client).post({
      url: "/mcp/{name}/auth",
      ...options
    });
  }
  /**
   * Complete OAuth authentication with authorization code
   */
  callback(options) {
    return (options.client ?? this._client).post({
      url: "/mcp/{name}/auth/callback",
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options.headers
      }
    });
  }
  /**
   * Start OAuth flow and wait for callback (opens browser)
   */
  authenticate(options) {
    return (options.client ?? this._client).post({
      url: "/mcp/{name}/auth/authenticate",
      ...options
    });
  }
  /**
   * Set authentication credentials
   */
  set(options) {
    return (options.client ?? this._client).put({
      url: "/auth/{id}",
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options.headers
      }
    });
  }
};
var Mcp = class extends _HeyApiClient {
  /**
   * Get MCP server status
   */
  status(options) {
    return (options?.client ?? this._client).get({
      url: "/mcp",
      ...options
    });
  }
  /**
   * Add MCP server dynamically
   */
  add(options) {
    return (options?.client ?? this._client).post({
      url: "/mcp",
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options?.headers
      }
    });
  }
  /**
   * Connect an MCP server
   */
  connect(options) {
    return (options.client ?? this._client).post({
      url: "/mcp/{name}/connect",
      ...options
    });
  }
  /**
   * Disconnect an MCP server
   */
  disconnect(options) {
    return (options.client ?? this._client).post({
      url: "/mcp/{name}/disconnect",
      ...options
    });
  }
  auth = new Auth({ client: this._client });
};
var Lsp = class extends _HeyApiClient {
  /**
   * Get LSP server status
   */
  status(options) {
    return (options?.client ?? this._client).get({
      url: "/lsp",
      ...options
    });
  }
};
var Formatter = class extends _HeyApiClient {
  /**
   * Get formatter status
   */
  status(options) {
    return (options?.client ?? this._client).get({
      url: "/formatter",
      ...options
    });
  }
};
var Control = class extends _HeyApiClient {
  /**
   * Get the next TUI request from the queue
   */
  next(options) {
    return (options?.client ?? this._client).get({
      url: "/tui/control/next",
      ...options
    });
  }
  /**
   * Submit a response to the TUI request queue
   */
  response(options) {
    return (options?.client ?? this._client).post({
      url: "/tui/control/response",
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options?.headers
      }
    });
  }
};
var Tui = class extends _HeyApiClient {
  /**
   * Append prompt to the TUI
   */
  appendPrompt(options) {
    return (options?.client ?? this._client).post({
      url: "/tui/append-prompt",
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options?.headers
      }
    });
  }
  /**
   * Open the help dialog
   */
  openHelp(options) {
    return (options?.client ?? this._client).post({
      url: "/tui/open-help",
      ...options
    });
  }
  /**
   * Open the session dialog
   */
  openSessions(options) {
    return (options?.client ?? this._client).post({
      url: "/tui/open-sessions",
      ...options
    });
  }
  /**
   * Open the theme dialog
   */
  openThemes(options) {
    return (options?.client ?? this._client).post({
      url: "/tui/open-themes",
      ...options
    });
  }
  /**
   * Open the model dialog
   */
  openModels(options) {
    return (options?.client ?? this._client).post({
      url: "/tui/open-models",
      ...options
    });
  }
  /**
   * Submit the prompt
   */
  submitPrompt(options) {
    return (options?.client ?? this._client).post({
      url: "/tui/submit-prompt",
      ...options
    });
  }
  /**
   * Clear the prompt
   */
  clearPrompt(options) {
    return (options?.client ?? this._client).post({
      url: "/tui/clear-prompt",
      ...options
    });
  }
  /**
   * Execute a TUI command (e.g. agent_cycle)
   */
  executeCommand(options) {
    return (options?.client ?? this._client).post({
      url: "/tui/execute-command",
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options?.headers
      }
    });
  }
  /**
   * Show a toast notification in the TUI
   */
  showToast(options) {
    return (options?.client ?? this._client).post({
      url: "/tui/show-toast",
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options?.headers
      }
    });
  }
  /**
   * Publish a TUI event
   */
  publish(options) {
    return (options?.client ?? this._client).post({
      url: "/tui/publish",
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options?.headers
      }
    });
  }
  control = new Control({ client: this._client });
};
var Event = class extends _HeyApiClient {
  /**
   * Get events
   */
  subscribe(options) {
    return (options?.client ?? this._client).get.sse({
      url: "/event",
      ...options
    });
  }
};
var OpencodeClient = class extends _HeyApiClient {
  /**
   * Respond to a permission request
   */
  postSessionIdPermissionsPermissionId(options) {
    return (options.client ?? this._client).post({
      url: "/session/{id}/permissions/{permissionID}",
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options.headers
      }
    });
  }
  global = new Global({ client: this._client });
  project = new Project({ client: this._client });
  pty = new Pty({ client: this._client });
  config = new Config({ client: this._client });
  tool = new Tool({ client: this._client });
  instance = new Instance({ client: this._client });
  path = new Path({ client: this._client });
  vcs = new Vcs({ client: this._client });
  session = new Session({ client: this._client });
  command = new Command({ client: this._client });
  provider = new Provider({ client: this._client });
  find = new Find({ client: this._client });
  file = new File({ client: this._client });
  app = new App({ client: this._client });
  mcp = new Mcp({ client: this._client });
  lsp = new Lsp({ client: this._client });
  formatter = new Formatter({ client: this._client });
  tui = new Tui({ client: this._client });
  auth = new Auth({ client: this._client });
  event = new Event({ client: this._client });
};

// node_modules/@opencode-ai/sdk/dist/client.js
function createOpencodeClient(config) {
  if (!config?.fetch) {
    const customFetch = (req) => {
      req.timeout = false;
      return fetch(req);
    };
    config = {
      ...config,
      fetch: customFetch
    };
  }
  if (config?.directory) {
    config.headers = {
      ...config.headers,
      "x-opencode-directory": encodeURIComponent(config.directory)
    };
  }
  const client2 = createClient(config);
  return new OpencodeClient({ client: client2 });
}

// src/capabilities.ts
var DISABLED_REASON_MESSAGES = {
  platform_unsupported: "Session Memory supports macOS only in v1 (platform_unsupported).",
  session_api_unavailable: "Required OpenCode session APIs are unreachable in this runtime (session_api_unavailable)."
};
function probeRuntimeCapabilities(input = {}) {
  const platform = input.platform ?? process.platform;
  if (platform !== "darwin") {
    return {
      state: "disabled",
      reason: "platform_unsupported",
      message: DISABLED_REASON_MESSAGES.platform_unsupported
    };
  }
  if (!hasUsableSessionApi(input.sessionApi)) {
    return {
      state: "disabled",
      reason: "session_api_unavailable",
      message: DISABLED_REASON_MESSAGES.session_api_unavailable
    };
  }
  return {
    state: "enabled",
    platform: "darwin",
    v1RamMetric: "rss_bytes"
  };
}
function hasUsableSessionApi(sessionApi) {
  if (!sessionApi || typeof sessionApi !== "object") {
    return false;
  }
  const candidate = sessionApi;
  return typeof candidate.list === "function" && typeof candidate.get === "function" && typeof candidate.messages === "function";
}

// src/live-sessions.ts
var INACTIVE_SESSION_STATUSES = /* @__PURE__ */ new Set([
  "inactive",
  "closed",
  "deleted",
  "historical",
  "archived",
  "terminated",
  "completed",
  "failed",
  "error"
]);
async function discoverLiveSessions(input) {
  const sessions = normalizeSessionList(await listSessionsAcrossRuntime(input.sessionApi));
  const currentSessionId = await resolveCurrentSessionId(input);
  const liveRows = sessions.filter((session) => isLiveStatus(session.status)).map((session) => ({
    sessionId: session.id,
    status: session.status,
    isCurrent: currentSessionId === session.id,
    projectId: session.projectId,
    projectPath: session.projectPath
  }));
  return stablePinCurrentFirst(liveRows);
}
function normalizeSessionList(raw) {
  const payload = unwrapSessionArray(raw);
  const normalized = payload.map((item) => normalizeSession(item)).filter((item) => item !== null);
  return dedupeById(normalized);
}
function unwrapSessionArray(raw) {
  if (Array.isArray(raw)) {
    return raw;
  }
  if (!raw || typeof raw !== "object") {
    return [];
  }
  const candidate = raw;
  if (Array.isArray(candidate.sessions)) {
    return candidate.sessions;
  }
  if (Array.isArray(candidate.items)) {
    return candidate.items;
  }
  if (Array.isArray(candidate.data)) {
    return candidate.data;
  }
  return [];
}
function normalizeSession(raw) {
  if (!raw || typeof raw !== "object") {
    return null;
  }
  const candidate = raw;
  const id = asString(candidate.id) ?? asString(candidate.sessionId) ?? asString(candidate.session_id);
  if (!id) {
    return null;
  }
  return {
    id,
    status: normalizeStatus(candidate.status),
    projectId: asString(candidate.projectId) ?? asString(candidate.project_id),
    projectPath: asString(candidate.projectPath) ?? asString(candidate.project_path)
  };
}
function normalizeStatus(raw) {
  if (typeof raw === "string") {
    return raw.trim().toLowerCase();
  }
  if (!raw || typeof raw !== "object") {
    return "unknown";
  }
  const candidate = raw;
  const nested = asString(candidate.type) ?? asString(candidate.state) ?? asString(candidate.status);
  return nested ? nested.trim().toLowerCase() : "unknown";
}
function isLiveStatus(status) {
  return !INACTIVE_SESSION_STATUSES.has(status);
}
async function listSessionsAcrossRuntime(sessionApi) {
  const listCalls = [
    () => sessionApi.list({ scope: "all", status: "live" }),
    () => sessionApi.list({ scope: "all", status: "open" }),
    () => sessionApi.list({ all: true }),
    () => sessionApi.list()
  ];
  for (const call of listCalls) {
    try {
      const value = await call();
      const asArray = unwrapSessionArray(value);
      if (asArray.length > 0) {
        return value;
      }
      if (Array.isArray(value)) {
        return value;
      }
    } catch {
      continue;
    }
  }
  return [];
}
async function resolveCurrentSessionId(input) {
  if (input.currentSessionId) {
    return input.currentSessionId;
  }
  try {
    const current = await input.sessionApi.get();
    return normalizeCurrentSessionId(current);
  } catch {
    return void 0;
  }
}
function normalizeCurrentSessionId(raw) {
  if (!raw || typeof raw !== "object") {
    return void 0;
  }
  const candidate = raw;
  if (asString(candidate.id)) {
    return asString(candidate.id);
  }
  if (asString(candidate.sessionId)) {
    return asString(candidate.sessionId);
  }
  if (asString(candidate.session_id)) {
    return asString(candidate.session_id);
  }
  if (candidate.session && typeof candidate.session === "object") {
    const nested = candidate.session;
    return asString(nested.id) ?? asString(nested.sessionId) ?? asString(nested.session_id);
  }
  return void 0;
}
function stablePinCurrentFirst(rows) {
  return [...rows].sort((left, right) => {
    if (left.isCurrent && !right.isCurrent) {
      return -1;
    }
    if (!left.isCurrent && right.isCurrent) {
      return 1;
    }
    const projectCompare = (left.projectPath ?? left.projectId ?? "").localeCompare(
      right.projectPath ?? right.projectId ?? ""
    );
    if (projectCompare !== 0) {
      return projectCompare;
    }
    return left.sessionId.localeCompare(right.sessionId);
  });
}
function asString(value) {
  return typeof value === "string" && value.length > 0 ? value : void 0;
}
function dedupeById(input) {
  const seen = /* @__PURE__ */ new Set();
  const output = [];
  for (const session of input) {
    if (seen.has(session.id)) {
      continue;
    }
    seen.add(session.id);
    output.push(session);
  }
  return output;
}

// src/process-macos.ts
import { execFile } from "node:child_process";
import { promisify } from "node:util";
var execFileAsync = promisify(execFile);
var DEFAULT_PROCESS_SAMPLE_TIMEOUT_MS = 1e3;
var MAX_PROCESS_SAMPLE_TIMEOUT_MS = 5e3;
var PS_SAMPLE_PATTERN = /^\s*(\d+)\s+([A-Za-z]{3}\s+[A-Za-z]{3}\s+\d{1,2}\s+\d{2}:\d{2}:\d{2}\s+\d{4})\s+(\d+)\s*$/;
function resolveProcessSampleTimeoutMs(timeoutMs) {
  if (!Number.isFinite(timeoutMs) || timeoutMs === void 0 || timeoutMs <= 0) {
    return DEFAULT_PROCESS_SAMPLE_TIMEOUT_MS;
  }
  const rounded = Math.floor(timeoutMs);
  return Math.max(1, Math.min(MAX_PROCESS_SAMPLE_TIMEOUT_MS, rounded));
}
function parsePsSampleOutput(stdout) {
  const firstLine = stdout.split(/\r?\n/).map((line) => line.trim()).find((line) => line.length > 0);
  if (!firstLine) {
    return null;
  }
  const match = PS_SAMPLE_PATTERN.exec(firstLine);
  if (!match) {
    return null;
  }
  const pid = Number.parseInt(match[1], 10);
  const startTimeIso = parsePsStartTimeToIso(match[2]);
  const rssKib = Number.parseInt(match[3], 10);
  if (!Number.isInteger(pid) || pid <= 0 || !startTimeIso || !Number.isFinite(rssKib) || rssKib < 0) {
    return null;
  }
  const rssBytes = rssKib * 1024;
  if (!Number.isSafeInteger(rssBytes)) {
    return null;
  }
  return {
    pid,
    startTimeIso,
    rssBytes
  };
}
async function sampleMacOsProcess(input) {
  const now = input.now ?? (() => /* @__PURE__ */ new Date());
  const sampledAtIso = now().toISOString();
  if (!Number.isInteger(input.pid) || input.pid <= 0) {
    return {
      state: "parse_failure",
      pid: input.pid,
      sampledAtIso,
      detail: "pid must be a positive integer"
    };
  }
  const timeoutMs = resolveProcessSampleTimeoutMs(input.timeoutMs);
  const runPsCommand = input.runPsCommand ?? runPsCommandWithExecFile;
  const commandResult = await runPsCommand(input.pid, timeoutMs);
  if (commandResult.state !== "ok") {
    return {
      state: commandResult.state,
      pid: input.pid,
      sampledAtIso,
      detail: commandResult.detail
    };
  }
  if (isPermissionDeniedText(commandResult.stderr)) {
    return {
      state: "permission_denied",
      pid: input.pid,
      sampledAtIso,
      detail: normalizeDetail(commandResult.stderr)
    };
  }
  const parsed = parsePsSampleOutput(commandResult.stdout);
  if (!parsed || parsed.pid !== input.pid) {
    const detail = normalizeDetail(commandResult.stderr);
    return {
      state: "parse_failure",
      pid: input.pid,
      sampledAtIso,
      detail
    };
  }
  return {
    state: "sampled",
    metric: "rss_bytes",
    bytes: parsed.rssBytes,
    identity: {
      pid: parsed.pid,
      startTimeIso: parsed.startTimeIso
    },
    sampledAtIso
  };
}
async function runPsCommandWithExecFile(pid, timeoutMs) {
  try {
    const { stdout, stderr } = await execFileAsync(
      "ps",
      ["-o", "pid=", "-o", "lstart=", "-o", "rss=", "-p", String(pid)],
      {
        encoding: "utf8",
        timeout: timeoutMs,
        maxBuffer: 64 * 1024,
        env: {
          ...process.env,
          LC_ALL: "C",
          LANG: "C"
        }
      }
    );
    return {
      state: "ok",
      stdout: toText(stdout),
      stderr: toText(stderr)
    };
  } catch (error) {
    return mapExecFileFailure(error);
  }
}
function parsePsStartTimeToIso(startTimeText) {
  const normalized = startTimeText.replace(/\s+/g, " ").trim();
  const parsedMs = Date.parse(normalized);
  if (!Number.isFinite(parsedMs)) {
    return null;
  }
  return new Date(parsedMs).toISOString();
}
function mapExecFileFailure(error) {
  const failure = error;
  const stdout = toText(failure.stdout);
  const stderr = toText(failure.stderr);
  const detail = normalizeDetail(failure.message);
  const combined = `${detail ?? ""}
${stderr}`;
  if (failure.code === "ETIMEDOUT" || failure.killed) {
    return {
      state: "timeout",
      stdout,
      stderr,
      detail
    };
  }
  if (isPermissionDeniedText(combined)) {
    return {
      state: "permission_denied",
      stdout,
      stderr,
      detail
    };
  }
  if (isNotFoundText(combined) || isEmptyResultForMissingPid(failure, stdout, stderr)) {
    return {
      state: "not_found",
      stdout,
      stderr,
      detail
    };
  }
  return {
    state: "command_error",
    stdout,
    stderr,
    detail
  };
}
function toText(value) {
  if (typeof value === "string") {
    return value;
  }
  if (value instanceof Buffer) {
    return value.toString("utf8");
  }
  if (value === void 0 || value === null) {
    return "";
  }
  return String(value);
}
function normalizeDetail(value) {
  const normalized = toText(value).trim();
  return normalized.length > 0 ? normalized : void 0;
}
function isPermissionDeniedText(value) {
  const normalized = value.toLowerCase();
  return normalized.includes("operation not permitted") || normalized.includes("permission denied");
}
function isNotFoundText(value) {
  const normalized = value.toLowerCase();
  return normalized.includes("no such process") || normalized.includes("not found");
}
function isEmptyResultForMissingPid(failure, stdout, stderr) {
  return typeof failure.code === "number" && failure.code !== 0 && stdout.trim().length === 0 && stderr.trim().length === 0;
}

// src/types.ts
function createRamState(input) {
  if (input.mappingState === "exact") {
    if (!Number.isFinite(input.bytes) || input.bytes < 0) {
      throw new Error("exact RAM state requires a non-negative finite byte value");
    }
    return {
      mappingState: "exact",
      bytes: input.bytes,
      metric: input.metric ?? "rss_bytes",
      sampledAtIso: input.sampledAtIso
    };
  }
  if ("bytes" in input && typeof input.bytes === "number") {
    throw new Error("numeric RAM is only valid when mappingState is exact");
  }
  return {
    mappingState: "unavailable",
    reason: input.reason,
    sampledAtIso: input.sampledAtIso
  };
}

// src/ram-attribution.ts
async function attributeSessionRam(input) {
  const now = input.now ?? (() => /* @__PURE__ */ new Date());
  const fallbackSampledAtIso = now().toISOString();
  const sampleTimeoutMs = resolveProcessSampleTimeoutMs(input.sampleTimeoutMs);
  const sampleProcess = input.sampleProcess ?? sampleMacOsProcess;
  const sharedPids = collectSharedPids(input.sessions);
  const sampleByPid = /* @__PURE__ */ new Map();
  const rows = [];
  for (const session of input.sessions) {
    const pid = session.pid;
    if (!isUsablePid(pid)) {
      rows.push({
        sessionId: session.sessionId,
        ram: createRamState({
          mappingState: "unavailable",
          reason: "unavailable_no_pid",
          sampledAtIso: fallbackSampledAtIso
        })
      });
      continue;
    }
    const sessionWithPid = {
      ...session,
      pid
    };
    if (sharedPids.has(pid)) {
      rows.push({
        sessionId: session.sessionId,
        ram: createRamState({
          mappingState: "unavailable",
          reason: "unavailable_shared_process",
          sampledAtIso: fallbackSampledAtIso
        })
      });
      continue;
    }
    const expectedStartTimeIso = normalizeStartTimeIso(session.startTimeIso);
    if (!expectedStartTimeIso) {
      rows.push({
        sessionId: session.sessionId,
        ram: createRamState({
          mappingState: "unavailable",
          reason: "stale",
          sampledAtIso: pickStaleTimestamp({
            session: sessionWithPid,
            fallbackSampledAtIso,
            previousExactSamplesBySessionId: input.previousExactSamplesBySessionId
          })
        })
      });
      continue;
    }
    const cachedSamplePromise = sampleByPid.get(pid);
    const samplePromise = cachedSamplePromise ?? sampleProcess({
      pid,
      timeoutMs: sampleTimeoutMs,
      now
    });
    if (!cachedSamplePromise) {
      sampleByPid.set(pid, samplePromise);
    }
    const sample = await samplePromise;
    rows.push({
      sessionId: session.sessionId,
      ram: classifySampleForSession({
        session: sessionWithPid,
        sample,
        expectedStartTimeIso,
        previousExactSamplesBySessionId: input.previousExactSamplesBySessionId
      })
    });
  }
  return rows;
}
function classifySampleForSession(input) {
  if (input.sample.state === "sampled") {
    const observedStartTimeIso = normalizeStartTimeIso(input.sample.identity.startTimeIso);
    const isSameInstance = input.sample.identity.pid === input.session.pid && observedStartTimeIso === input.expectedStartTimeIso;
    if (isSameInstance) {
      return createRamState({
        mappingState: "exact",
        metric: input.sample.metric,
        bytes: input.sample.bytes,
        sampledAtIso: input.sample.sampledAtIso
      });
    }
    return createRamState({
      mappingState: "unavailable",
      reason: "stale",
      sampledAtIso: pickStaleTimestamp({
        session: input.session,
        expectedStartTimeIso: input.expectedStartTimeIso,
        fallbackSampledAtIso: input.sample.sampledAtIso,
        previousExactSamplesBySessionId: input.previousExactSamplesBySessionId
      })
    });
  }
  if (input.sample.state === "permission_denied") {
    return createRamState({
      mappingState: "unavailable",
      reason: "permission_denied",
      sampledAtIso: input.sample.sampledAtIso
    });
  }
  return createRamState({
    mappingState: "unavailable",
    reason: "stale",
    sampledAtIso: pickStaleTimestamp({
      session: input.session,
      expectedStartTimeIso: input.expectedStartTimeIso,
      fallbackSampledAtIso: input.sample.sampledAtIso,
      previousExactSamplesBySessionId: input.previousExactSamplesBySessionId
    })
  });
}
function isUsablePid(pid) {
  return typeof pid === "number" && Number.isInteger(pid) && pid > 0;
}
function normalizeStartTimeIso(startTimeIso) {
  if (!startTimeIso) {
    return null;
  }
  const parsed = Date.parse(startTimeIso);
  if (!Number.isFinite(parsed)) {
    return null;
  }
  return new Date(parsed).toISOString();
}
function collectSharedPids(sessions) {
  const countsByPid = /* @__PURE__ */ new Map();
  for (const session of sessions) {
    if (!isUsablePid(session.pid)) {
      continue;
    }
    countsByPid.set(session.pid, (countsByPid.get(session.pid) ?? 0) + 1);
  }
  return new Set(
    Array.from(countsByPid.entries()).filter(([, count]) => count > 1).map(([pid]) => pid)
  );
}
function pickStaleTimestamp(input) {
  const previous = input.previousExactSamplesBySessionId?.get(input.session.sessionId);
  if (!previous || previous.pid !== input.session.pid) {
    return input.fallbackSampledAtIso;
  }
  if (input.expectedStartTimeIso) {
    const previousStartTimeIso = normalizeStartTimeIso(previous.startTimeIso);
    if (previousStartTimeIso !== input.expectedStartTimeIso) {
      return input.fallbackSampledAtIso;
    }
  }
  return previous.sampledAtIso;
}

// src/token-usage.ts
var EMPTY_TOKEN_USAGE = {
  inputTokens: 0,
  outputTokens: 0,
  reasoningTokens: 0,
  cacheReadTokens: 0,
  totalTokens: 0
};
function aggregateSessionTokenUsage(messages) {
  let inputTokens = 0;
  let outputTokens = 0;
  let reasoningTokens = 0;
  let cacheReadTokens = 0;
  for (const message of messages) {
    if (!isAssistantMessage(message)) {
      continue;
    }
    const usage = extractUsageBuckets(message);
    inputTokens += usage.inputTokens;
    outputTokens += usage.outputTokens;
    reasoningTokens += usage.reasoningTokens;
    cacheReadTokens += usage.cacheReadTokens;
  }
  const totalTokens = inputTokens + outputTokens + reasoningTokens + cacheReadTokens;
  return {
    inputTokens,
    outputTokens,
    reasoningTokens,
    cacheReadTokens,
    totalTokens
  };
}
function isAssistantMessage(message) {
  if (!message || typeof message !== "object") {
    return false;
  }
  const candidate = message;
  const role = asString2(candidate.role) ?? asString2(candidate.type) ?? "";
  return role.toLowerCase() === "assistant";
}
function extractUsageBuckets(message) {
  const usageScopes = gatherUsageScopes(message);
  const inputTokens = readFirstNumber(usageScopes, [
    ["inputTokens"],
    ["input_tokens"],
    ["input"],
    ["promptTokens"],
    ["prompt_tokens"],
    ["prompt"]
  ]);
  const outputTokens = readFirstNumber(usageScopes, [
    ["outputTokens"],
    ["output_tokens"],
    ["output"],
    ["completionTokens"],
    ["completion_tokens"],
    ["completion"]
  ]);
  const reasoningTokens = readFirstNumber(usageScopes, [
    ["reasoningTokens"],
    ["reasoning_tokens"],
    ["reasoning"]
  ]);
  const cacheReadTokens = readFirstNumber(usageScopes, [
    ["cacheReadTokens"],
    ["cache_read_tokens"],
    ["cacheReadInputTokens"],
    ["cache_read_input_tokens"],
    ["cacheRead"],
    ["cache", "read"]
  ]);
  return {
    inputTokens,
    outputTokens,
    reasoningTokens,
    cacheReadTokens
  };
}
function gatherUsageScopes(message) {
  if (!message || typeof message !== "object") {
    return [];
  }
  const candidate = message;
  const scopes = [];
  if (isRecord(candidate)) {
    scopes.push(candidate);
  }
  if (isRecord(candidate.tokens)) {
    scopes.push(candidate.tokens);
  }
  if (isRecord(candidate.usage)) {
    scopes.push(candidate.usage);
  }
  if (isRecord(candidate.metrics)) {
    scopes.push(candidate.metrics);
    if (isRecord(candidate.metrics.tokens)) {
      scopes.push(candidate.metrics.tokens);
    }
  }
  return scopes;
}
function readFirstNumber(scopes, paths) {
  for (const path of paths) {
    for (const scope of scopes) {
      const value = readPath(scope, path);
      if (isFiniteNonNegativeNumber(value)) {
        return value;
      }
    }
  }
  return 0;
}
function readPath(record, path) {
  let current = record;
  for (const key of path) {
    if (!current || typeof current !== "object") {
      return void 0;
    }
    current = current[key];
  }
  return current;
}
function isFiniteNonNegativeNumber(value) {
  return typeof value === "number" && Number.isFinite(value) && value >= 0;
}
function asString2(value) {
  return typeof value === "string" && value.length > 0 ? value : void 0;
}
function isRecord(value) {
  return !!value && typeof value === "object";
}

// src/sidebar.ts
var SIDEBAR_PANEL_TITLE = "Session Memory";
var SIDEBAR_POLL_INTERVAL_MS = 5e3;
var FALLBACK_UNAVAILABLE_LABEL = "unavailable_unknown";
async function buildSessionMemorySnapshot(input) {
  const capability = probeRuntimeCapabilities(input.capabilityProbeInput);
  if (capability.state === "disabled") {
    return {
      capability,
      rows: []
    };
  }
  const collectRows = input.collectRows ?? collectSessionMemoryRows;
  const rows = await collectRows({
    sessionApi: input.sessionApi,
    currentSessionId: input.currentSessionId
  });
  return {
    capability,
    rows
  };
}
function buildSidebarPanelModel(snapshot) {
  const orderedRows = sortRowsCurrentFirst(snapshot.rows);
  const summary = summarizeRows(orderedRows);
  const current = orderedRows.find((row) => row.isCurrent) ?? null;
  const others = orderedRows.filter((row) => !row.isCurrent);
  return {
    title: SIDEBAR_PANEL_TITLE,
    pollIntervalMs: SIDEBAR_POLL_INTERVAL_MS,
    capability: snapshot.capability,
    summary,
    current: current ? toSidebarRowView(current) : null,
    others: others.map(toSidebarRowView)
  };
}
function buildSidebarItems(model) {
  const items = [];
  items.push({ id: "summary.live", label: "Live", value: String(model.summary.liveSessionCount) });
  items.push({
    id: "summary.exact",
    label: "Exact RAM",
    value: `${model.summary.exactRamCoverageCount}/${model.summary.liveSessionCount}`
  });
  items.push({
    id: "summary.total",
    label: "Exact Total",
    value: formatBytes(model.summary.exactRamTotalBytes)
  });
  items.push({
    id: "summary.unavailable",
    label: "Unavailable",
    value: String(model.summary.unavailableRamCount)
  });
  if (model.capability.state === "disabled") {
    items.push(...disabledItems(model.capability));
    return items;
  }
  if (model.current) {
    items.push({
      id: `current.${model.current.sessionId}`,
      label: `Current ${model.current.sessionId}`,
      value: `tokens ${model.current.tokensTotal} | RAM ${model.current.ramLabel}`
    });
  } else {
    items.push({
      id: "current.none",
      label: "Current",
      value: "none"
    });
  }
  for (const row of model.others) {
    items.push({
      id: `other.${row.sessionId}`,
      label: `Other ${row.sessionId}`,
      value: `tokens ${row.tokensTotal} | RAM ${row.ramLabel}`
    });
  }
  return items;
}
function createSessionMemorySidebarDefinition(input) {
  return {
    id: input.id ?? "session-memory",
    title: SIDEBAR_PANEL_TITLE,
    items: async () => {
      const snapshot = await input.snapshot();
      const model = buildSidebarPanelModel(snapshot);
      return buildSidebarItems(model);
    }
  };
}
async function collectSessionMemoryRows(input) {
  const liveSessions = await discoverLiveSessions({
    sessionApi: input.sessionApi,
    currentSessionId: input.currentSessionId
  });
  const messageResults = await Promise.all(
    liveSessions.map(async (session) => ({
      sessionId: session.sessionId,
      messages: await readSessionMessages(input.sessionApi, session.sessionId)
    }))
  );
  const sessionMappings = await Promise.all(
    liveSessions.map((session) => resolveProcessMappingCandidate(input.sessionApi, session))
  );
  const ramRows = await attributeSessionRam({
    sessions: sessionMappings,
    ...input.ramAttributionOverrides
  });
  const ramBySessionId = new Map(ramRows.map((row) => [row.sessionId, row.ram]));
  const messagesBySessionId = new Map(messageResults.map((row) => [row.sessionId, row.messages]));
  return liveSessions.map((session) => {
    const messages = messagesBySessionId.get(session.sessionId) ?? [];
    return {
      sessionId: session.sessionId,
      isCurrent: session.isCurrent,
      tokenUsage: aggregateSessionTokenUsage(messages),
      ram: ramBySessionId.get(session.sessionId) ?? {
        mappingState: "unavailable",
        reason: "unavailable_no_pid",
        sampledAtIso: (/* @__PURE__ */ new Date()).toISOString()
      }
    };
  });
}
async function readSessionMessages(sessionApi, sessionId) {
  const calls = [
    () => sessionApi.messages({ sessionId }),
    () => sessionApi.messages({ id: sessionId }),
    () => sessionApi.messages(sessionId)
  ];
  for (const call of calls) {
    try {
      const result = await call();
      const normalized = normalizeMessageArray(result);
      if (normalized.length > 0 || Array.isArray(result)) {
        return normalized;
      }
    } catch {
      continue;
    }
  }
  return [];
}
function normalizeMessageArray(raw) {
  if (Array.isArray(raw)) {
    return raw;
  }
  if (!raw || typeof raw !== "object") {
    return [];
  }
  const candidate = raw;
  if (Array.isArray(candidate.messages)) {
    return candidate.messages;
  }
  if (Array.isArray(candidate.items)) {
    return candidate.items;
  }
  if (Array.isArray(candidate.data)) {
    return candidate.data;
  }
  return [];
}
async function resolveProcessMappingCandidate(sessionApi, session) {
  const fallback = {
    sessionId: session.sessionId
  };
  const calls = [
    () => sessionApi.get({ sessionId: session.sessionId }),
    () => sessionApi.get({ id: session.sessionId }),
    () => sessionApi.get(session.sessionId)
  ];
  for (const call of calls) {
    try {
      const detail = await call();
      const mapping = extractMappingFromSessionDetail(detail, session.sessionId);
      if (mapping.pid) {
        return mapping;
      }
    } catch {
      continue;
    }
  }
  return fallback;
}
function extractMappingFromSessionDetail(detail, sessionId) {
  const records = flattenRecords(detail);
  let pid;
  let startTimeIso;
  for (const record of records) {
    pid = pid ?? firstFiniteInt(record, ["pid", "processPid", "process_pid"]);
    startTimeIso = startTimeIso ?? firstString(record, [
      "startTimeIso",
      "startedAtIso",
      "start_time_iso",
      "startTime",
      "startedAt"
    ]);
  }
  return {
    sessionId,
    pid,
    startTimeIso
  };
}
function flattenRecords(raw) {
  if (!raw || typeof raw !== "object") {
    return [];
  }
  const queue = [raw];
  const output = [];
  while (queue.length > 0) {
    const current = queue.shift();
    if (!current || typeof current !== "object") {
      continue;
    }
    const record = current;
    output.push(record);
    for (const value of Object.values(record)) {
      if (value && typeof value === "object") {
        queue.push(value);
      }
    }
  }
  return output;
}
function firstFiniteInt(record, keys) {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === "number" && Number.isFinite(value) && Number.isInteger(value) && value > 0) {
      return value;
    }
  }
  return void 0;
}
function firstString(record, keys) {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === "string" && value.length > 0) {
      return value;
    }
  }
  return void 0;
}
function summarizeRows(rows) {
  let exactRamCoverageCount = 0;
  let exactRamTotalBytes = 0;
  let unavailableRamCount = 0;
  for (const row of rows) {
    if (row.ram.mappingState === "exact") {
      exactRamCoverageCount += 1;
      exactRamTotalBytes += row.ram.bytes;
      continue;
    }
    unavailableRamCount += 1;
  }
  return {
    liveSessionCount: rows.length,
    exactRamCoverageCount,
    unavailableRamCount,
    exactRamTotalBytes
  };
}
function sortRowsCurrentFirst(rows) {
  return [...rows].sort((left, right) => {
    if (left.isCurrent && !right.isCurrent) {
      return -1;
    }
    if (!left.isCurrent && right.isCurrent) {
      return 1;
    }
    return left.sessionId.localeCompare(right.sessionId);
  });
}
function toSidebarRowView(row) {
  return {
    sessionId: row.sessionId,
    isCurrent: row.isCurrent,
    tokensTotal: row.tokenUsage.totalTokens,
    ramLabel: ramStateToLabel(row.ram)
  };
}
function ramStateToLabel(ram) {
  if (ram.mappingState === "exact") {
    return formatBytes(ram.bytes);
  }
  return `unavailable (${ram.reason ?? FALLBACK_UNAVAILABLE_LABEL})`;
}
function disabledItems(disabled) {
  return [
    {
      id: "disabled.reason",
      label: "Disabled",
      value: disabled.reason
    },
    {
      id: "disabled.message",
      label: "Reason",
      value: disabled.message
    }
  ];
}
function formatBytes(bytes) {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return "0 B";
  }
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let value = bytes;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  const precision = value >= 100 ? 0 : value >= 10 ? 1 : 2;
  return `${value.toFixed(precision)} ${units[unitIndex]}`;
}

// src/index.ts
var DEFAULT_SERVER_URL = process.env.OPENCODE_SERVER_URL || "http://localhost:4096";
function createSessionMemoryTool(options = {}) {
  return {
    description: "Summarize live OpenCode session memory, token usage, and RAM attribution for the current session set.",
    args: {},
    execute: async (_args, context) => {
      const sessionApi = options.sessionApi ?? createRuntimeSessionApi(context, options.serverUrl);
      const capabilityProbeInput = {
        platform: options.platform ?? process.platform,
        sessionApi
      };
      const snapshot = await buildSessionMemorySnapshot({
        capabilityProbeInput,
        sessionApi,
        currentSessionId: context.sessionID
      });
      return formatSessionMemoryReport(buildSidebarPanelModel(snapshot));
    }
  };
}
var sessionMemoryTool = createSessionMemoryTool();
var index_default = sessionMemoryTool;
function createRuntimeSessionApi(context, serverUrl = DEFAULT_SERVER_URL) {
  const client2 = createOpencodeClient({
    baseUrl: serverUrl,
    directory: context.directory
  });
  return {
    list: (input) => client2.session.list({ query: input }),
    get: (input) => {
      if (!input || typeof input !== "object") {
        return client2.session.status();
      }
      const record = input;
      const id = typeof record.id === "string" && record.id || typeof record.sessionId === "string" && record.sessionId || typeof record.session_id === "string" && record.session_id || void 0;
      return id ? client2.session.get({ path: { id } }) : client2.session.status();
    },
    messages: (input) => {
      const id = resolveSessionId(input);
      if (!id) {
        return Promise.resolve([]);
      }
      return client2.session.messages({ path: { id } });
    }
  };
}
function resolveSessionId(input) {
  if (typeof input === "string" && input.length > 0) {
    return input;
  }
  if (!input || typeof input !== "object") {
    return void 0;
  }
  const record = input;
  if (typeof record.sessionId === "string" && record.sessionId.length > 0) {
    return record.sessionId;
  }
  if (typeof record.id === "string" && record.id.length > 0) {
    return record.id;
  }
  if (typeof record.session_id === "string" && record.session_id.length > 0) {
    return record.session_id;
  }
  return void 0;
}
function formatSessionMemoryReport(model) {
  const items = buildSidebarItems(model);
  const lines = ["# Session Memory"];
  if (model.capability.state === "disabled") {
    lines.push(`Capability: disabled (${model.capability.reason})`);
    lines.push(model.capability.message);
    return lines.join("\n");
  }
  lines.push("Summary:");
  for (const item of items.filter((item2) => item2.id.startsWith("summary."))) {
    lines.push(`- ${item.label}: ${item.value ?? ""}`.trim());
  }
  if (model.current) {
    lines.push(`Current: ${model.current.sessionId} | tokens ${model.current.tokensTotal} | RAM ${model.current.ramLabel}`);
  }
  if (model.others.length > 0) {
    lines.push("Other live sessions:");
    for (const row of model.others) {
      lines.push(`- ${row.sessionId} | tokens ${row.tokensTotal} | RAM ${row.ramLabel}`);
    }
  }
  return lines.join("\n");
}
export {
  DISABLED_REASON_MESSAGES,
  EMPTY_TOKEN_USAGE,
  FALLBACK_UNAVAILABLE_LABEL,
  SIDEBAR_PANEL_TITLE,
  SIDEBAR_POLL_INTERVAL_MS,
  aggregateSessionTokenUsage,
  buildSessionMemorySnapshot,
  buildSidebarItems,
  buildSidebarPanelModel,
  collectSessionMemoryRows,
  createRamState,
  createSessionMemorySidebarDefinition,
  createSessionMemoryTool,
  index_default as default,
  discoverLiveSessions,
  hasUsableSessionApi,
  probeRuntimeCapabilities
};
