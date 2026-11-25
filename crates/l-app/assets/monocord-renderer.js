"use strict";
(() => {
  // src/core/events.ts
  var EventBus = class {
    listeners = /* @__PURE__ */ new Map();
    /** Subscribe. Returns an unsubscribe function. */
    on(event, handler, scope) {
      let set = this.listeners.get(event);
      if (!set) {
        set = /* @__PURE__ */ new Set();
        this.listeners.set(event, set);
      }
      const reg = { handler, scope };
      set.add(reg);
      return () => {
        set.delete(reg);
        if (set.size === 0) this.listeners.delete(event);
      };
    }
    /** Subscribe for a single emission. */
    once(event, handler, scope) {
      const off = this.on(
        event,
        (payload) => {
          off();
          handler(payload);
        },
        scope
      );
      return off;
    }
    /** Remove a specific handler. */
    off(event, handler) {
      const set = this.listeners.get(event);
      if (!set) return;
      for (const reg of set) {
        if (reg.handler === handler) set.delete(reg);
      }
      if (set.size === 0) this.listeners.delete(event);
    }
    /** Emit an event to all subscribers. Handler errors are isolated. */
    emit(event, payload) {
      const set = this.listeners.get(event);
      if (!set) return;
      for (const reg of [...set]) {
        try {
          reg.handler(payload);
        } catch (e) {
          console.error(`[Monocord:events] listener for "${event}" threw`, e);
        }
      }
    }
    /** Remove every listener registered under a scope (e.g. a plugin name). */
    disposeScope(scope) {
      let removed = 0;
      for (const [event, set] of this.listeners) {
        for (const reg of [...set]) {
          if (reg.scope === scope) {
            set.delete(reg);
            removed++;
          }
        }
        if (set.size === 0) this.listeners.delete(event);
      }
      return removed;
    }
    /** Count listeners for an event (debugging). */
    listenerCount(event) {
      return this.listeners.get(event)?.size ?? 0;
    }
  };
  var Events = new EventBus();
  function eventScope(name) {
    return {
      on: (event, handler) => Events.on(event, handler, name),
      once: (event, handler) => Events.once(event, handler, name),
      off: (event, handler) => Events.off(event, handler),
      dispose: () => {
        Events.disposeScope(name);
      }
    };
  }

  // src/core/logger.ts
  var LEVEL_ORDER = {
    debug: 10,
    info: 20,
    warn: 30,
    error: 40
  };
  var globalLevel = "info";
  var sink;
  function setLogSink(fn) {
    sink = fn;
  }
  var Logger = class _Logger {
    constructor(scope) {
      this.scope = scope;
    }
    static get(scope) {
      return new _Logger(scope);
    }
    child(name) {
      return new _Logger(`${this.scope}:${name}`);
    }
    emit(level, message, data) {
      if (LEVEL_ORDER[level] < LEVEL_ORDER[globalLevel]) return;
      const line = `[Monocord:${this.scope}] ${message}`;
      const consoleFn = level === "error" ? console.error : level === "warn" ? console.warn : console.log;
      if (data === void 0) consoleFn(line);
      else consoleFn(line, data);
      sink?.(level, this.scope, message, data);
    }
    debug(message, data) {
      this.emit("debug", message, data);
    }
    info(message, data) {
      this.emit("info", message, data);
    }
    warn(message, data) {
      this.emit("warn", message, data);
    }
    error(message, data) {
      this.emit("error", message, data);
    }
  };

  // src/core/errors.ts
  var FrameworkError = class extends Error {
    /** Machine-readable error code. */
    code;
    constructor(message, code = "FRAMEWORK_ERROR", cause) {
      super(message, cause === void 0 ? void 0 : { cause });
      this.name = "FrameworkError";
      this.code = code;
    }
  };
  var ModuleNotFoundError = class extends FrameworkError {
    constructor(what) {
      super(`webpack module not found: ${what}`, "MODULE_NOT_FOUND");
      this.name = "ModuleNotFoundError";
    }
  };

  // src/core/patcher/index.ts
  var stacks = /* @__PURE__ */ new WeakMap();
  var log = Logger.get("patcher");
  function stackFor(object, key, fn) {
    let byKey = stacks.get(object);
    if (!byKey) {
      byKey = /* @__PURE__ */ new Map();
      stacks.set(object, byKey);
    }
    let stack = byKey.get(key);
    if (!stack) {
      stack = { object, key, original: fn, entries: [] };
      byKey.set(key, stack);
    }
    return stack;
  }
  function wrap(inner, entry) {
    const { mode, callback } = entry;
    if (mode === "before") {
      return function(...args) {
        const replacement = callback.apply(this, args);
        const finalArgs = Array.isArray(replacement) ? replacement : args;
        return inner.apply(this, finalArgs);
      };
    }
    if (mode === "after") {
      return function(...args) {
        const result = inner.apply(this, args);
        const replaced = callback.call(this, result, ...args);
        return replaced === void 0 ? result : replaced;
      };
    }
    return function(...args) {
      const result = callback.apply(this, args);
      return result === void 0 ? inner.apply(this, args) : result;
    };
  }
  function rebuild(stack) {
    if (stack.entries.length === 0) {
      stack.object[stack.key] = stack.original;
      return;
    }
    let fn = stack.original;
    for (const entry of stack.entries) fn = wrap(fn, entry);
    stack.object[stack.key] = fn;
  }
  function getFn(object, key) {
    const fn = object[key];
    if (typeof fn !== "function") {
      throw new FrameworkError(
        `cannot patch non-function ${String(key)}`,
        "PATCH_TARGET_NOT_FUNCTION"
      );
    }
    return fn;
  }
  function apply(plugin, object, key, mode, callback) {
    const current = getFn(object, key);
    const stack = stackFor(object, key, current);
    stack.entries.push({ plugin, mode, callback });
    let set = pluginStacks.get(plugin);
    if (!set) {
      set = /* @__PURE__ */ new Set();
      pluginStacks.set(plugin, set);
    }
    set.add(stack);
    rebuild(stack);
    log.debug(`${mode} patch applied`, { plugin, key: String(key) });
  }
  function removePluginFrom(stack, plugin) {
    const before = stack.entries.length;
    stack.entries = stack.entries.filter((e) => e.plugin !== plugin);
    return before - stack.entries.length;
  }
  function gcStack(stack) {
    if (stack.entries.length > 0) return;
    for (const set of pluginStacks.values()) set.delete(stack);
  }
  function unpatch(plugin, object, key) {
    const byKey = stacks.get(object);
    const stack = byKey?.get(key);
    if (!stack) return false;
    const removed = removePluginFrom(stack, plugin);
    if (removed === 0) return false;
    rebuild(stack);
    gcStack(stack);
    if (stack.entries.length === 0) byKey.delete(key);
    return true;
  }
  var Patcher = {
    /** Run `callback` before the original method. */
    before(plugin, object, key, callback) {
      apply(plugin, object, key, "before", callback);
    },
    /** Run `callback` after the original method. */
    after(plugin, object, key, callback) {
      apply(plugin, object, key, "after", callback);
    },
    /** Replace the method; `undefined` return falls through to original. */
    instead(plugin, object, key, callback) {
      apply(plugin, object, key, "instead", callback);
    },
    /** Remove all patches a plugin placed on `object.key`. */
    unpatch,
    /** Remove every patch belonging to a plugin, across all objects. */
    unpatchAll(plugin) {
      let removed = 0;
      for (const stack of pluginStacks.get(plugin) ?? []) {
        removed += removePluginFrom(stack, plugin);
        rebuild(stack);
      }
      pluginStacks.delete(plugin);
      return removed;
    },
    /** Patch records currently registered for a plugin (debugging). */
    patchesOf(plugin) {
      const out = [];
      for (const stack of pluginStacks.get(plugin) ?? []) {
        out.push(...stack.entries.filter((e) => e.plugin === plugin));
      }
      return out;
    }
  };
  var pluginStacks = /* @__PURE__ */ new Map();

  // src/core/settings.ts
  var log2 = Logger.get("settings");
  var MemoryBackend = class {
    data = {};
    async load() {
      return structuredClone(this.data);
    }
    async save(data) {
      this.data = structuredClone(data);
    }
  };
  var IpcBackend = class {
    constructor(bridge5) {
      this.bridge = bridge5;
    }
    async load() {
      const data = await this.bridge.invoke("settings-get");
      return data ?? {};
    }
    async save(data) {
      await this.bridge.invoke("settings-save", data);
    }
  };
  function getPath(obj, path) {
    let cur = obj;
    for (const part of path.split(".")) {
      if (cur === null || typeof cur !== "object") return void 0;
      cur = cur[part];
    }
    return cur;
  }
  function setPath(obj, path, value) {
    const parts = path.split(".");
    let cur = obj;
    for (let i = 0; i < parts.length - 1; i++) {
      const part = parts[i];
      const next = cur[part];
      if (next === null || typeof next !== "object" || Array.isArray(next)) {
        cur[part] = {};
      }
      cur = cur[part];
    }
    cur[parts[parts.length - 1]] = value;
  }
  var SettingsStore = class {
    constructor(backend = new MemoryBackend()) {
      this.backend = backend;
    }
    data = {};
    loaded = false;
    loadPromise = null;
    /** Swap the storage backend (e.g. memory → IPC once the bridge exists). */
    useBackend(backend) {
      this.backend = backend;
      this.loaded = false;
      this.loadPromise = null;
      this.data = {};
    }
    /** Lazily load persisted settings (once). */
    async ready() {
      if (this.loaded) return;
      if (!this.loadPromise) {
        this.loadPromise = this.backend.load().then((d) => {
          this.data = d ?? {};
          this.loaded = true;
        }).catch((e) => {
          log2.error("failed to load settings, using defaults", e);
          this.data = {};
          this.loaded = true;
        });
      }
      await this.loadPromise;
    }
    /** Read a dot-path value. */
    async get(path, fallback) {
      await this.ready();
      const v = getPath(this.data, path);
      return v === void 0 ? fallback : v;
    }
    /** Synchronous read — only valid after `ready()` resolved. */
    getSync(path, fallback) {
      const v = getPath(this.data, path);
      return v === void 0 ? fallback : v;
    }
    /** Write a dot-path value and persist. */
    async set(path, value) {
      await this.ready();
      setPath(this.data, path, value);
      Events.emit("settings.changed", { path, value });
      try {
        await this.backend.save(this.data);
      } catch (e) {
        log2.error(`failed to persist setting "${path}"`, e);
      }
    }
    /** Remove a dot-path value and persist. */
    async delete(path) {
      await this.ready();
      const parts = path.split(".");
      let cur = this.data;
      for (const part of parts.slice(0, -1)) {
        if (cur === null || typeof cur !== "object") return;
        cur = cur[part];
      }
      if (cur !== null && typeof cur === "object") {
        delete cur[parts[parts.length - 1]];
      }
      Events.emit("settings.changed", { path, value: void 0 });
      try {
        await this.backend.save(this.data);
      } catch (e) {
        log2.error(`failed to persist deletion of "${path}"`, e);
      }
    }
  };
  var Settings = new SettingsStore();

  // src/core/plugins/manager.ts
  var log3 = Logger.get("plugins");
  var SETTINGS_ENABLED = "plugins.enabled";
  function normalize(source) {
    const resolved = typeof source === "function" ? source() : source;
    const plugin = "default" in resolved ? resolved.default : resolved;
    if (!plugin || typeof plugin.name !== "string" || plugin.name.length === 0) {
      throw new FrameworkError("plugin is missing a name", "INVALID_PLUGIN");
    }
    return plugin;
  }
  var PluginManager = class {
    entries = /* @__PURE__ */ new Map();
    started = false;
    /** All registered plugin names. */
    names() {
      return [...this.entries.keys()];
    }
    get(name) {
      return this.entries.get(name)?.plugin;
    }
    isEnabled(name) {
      return this.entries.get(name)?.enabled ?? false;
    }
    isRunning(name) {
      const e = this.entries.get(name);
      return e?.enabled === true && !e.starting;
    }
    /** Register a plugin (does not start it). */
    register(source) {
      const plugin = normalize(source);
      if (this.entries.has(plugin.name)) {
        throw new FrameworkError(`duplicate plugin name: ${plugin.name}`, "DUPLICATE_PLUGIN");
      }
      this.entries.set(plugin.name, {
        plugin,
        enabled: false,
        scope: eventScope(plugin.name),
        starting: false
      });
      Events.emit("plugin.registered", { name: plugin.name });
      log3.debug("registered", { name: plugin.name });
      return plugin;
    }
    /** Unregister a plugin; disables it first if running. */
    async unregister(name) {
      const entry = this.entries.get(name);
      if (!entry) return;
      if (entry.enabled) await this.disable(name);
      entry.scope.dispose();
      this.entries.delete(name);
      Events.emit("plugin.unregistered", { name });
    }
    /** Enable a plugin, starting dependencies first. */
    async enable(name) {
      const entry = this.entries.get(name);
      if (!entry) throw new FrameworkError(`unknown plugin: ${name}`, "UNKNOWN_PLUGIN");
      if (entry.enabled) return;
      for (const dep of entry.plugin.required ?? []) {
        if (!this.entries.has(dep)) {
          throw new FrameworkError(
            `plugin ${name} requires missing plugin ${dep}`,
            "MISSING_DEPENDENCY"
          );
        }
        if (!this.isEnabled(dep)) await this.enable(dep);
      }
      entry.enabled = true;
      await this.persistEnabled();
      await this.startEntry(entry);
      Events.emit("plugin.enabled", { name });
    }
    /** Disable a plugin and undo its effects. */
    async disable(name) {
      const entry = this.entries.get(name);
      if (!entry || !entry.enabled) return;
      entry.enabled = false;
      await this.persistEnabled();
      await this.stopEntry(entry);
      Events.emit("plugin.disabled", { name });
    }
    /** Stop then start a plugin. */
    async restart(name) {
      const entry = this.entries.get(name);
      if (!entry?.enabled) return;
      await this.stopEntry(entry);
      await this.startEntry(entry);
    }
    /** Enable every plugin marked enabled in settings, in dependency order. */
    async startEnabled() {
      await Settings.ready();
      await this.startMatching((p) => p.defer !== true);
      this.started = true;
    }
    /**
     * Start enabled plugins flagged `defer`, yielding to the event loop
     * between each so Discord stays responsive while they install.
     */
    async startDeferred() {
      await this.startMatching((p) => p.defer === true, true);
    }
    /** Last start/stop error per plugin name, for diagnostics UI. */
    failures = /* @__PURE__ */ new Map();
    /** Plugin names whose most recent lifecycle call threw. */
    getFailures() {
      return [...this.failures.keys()];
    }
    async startMatching(match, yieldBetween = false) {
      const enabled = await Settings.get(SETTINGS_ENABLED, []) ?? [];
      for (const name of this.topoOrder()) {
        const entry = this.entries.get(name);
        if (!entry || entry.enabled !== false || !enabled.includes(name) || !match(entry.plugin)) {
          continue;
        }
        try {
          await this.enable(name);
          this.failures.delete(name);
        } catch (e) {
          this.failures.set(name, e);
          log3.error(`failed to enable ${name} at startup`, e);
          Events.emit("plugin.error", { name, phase: "start", error: e });
        }
        if (yieldBetween) {
          await new Promise((r) => setTimeout(r, 0));
        }
      }
    }
    /** Disable all running plugins (framework shutdown). */
    async stopAll() {
      for (const name of [...this.entries.keys()].reverse()) {
        await this.disable(name).catch(
          (e) => log3.error(`error disabling ${name}`, e)
        );
      }
      this.started = false;
    }
    /** Dependency-respecting start order over all registered plugins. */
    topoOrder() {
      const out = [];
      const state = /* @__PURE__ */ new Map();
      const visit = (name) => {
        const s = state.get(name);
        if (s === "done") return;
        if (s === "visiting") return;
        state.set(name, "visiting");
        const entry = this.entries.get(name);
        if (!entry) return;
        for (const dep of entry.plugin.required ?? []) {
          if (this.entries.has(dep)) visit(dep);
          else log3.warn(`plugin ${name} requires missing ${dep}`);
        }
        state.set(name, "done");
        out.push(name);
      };
      for (const name of this.entries.keys()) visit(name);
      return out;
    }
    async startEntry(entry) {
      if (!entry.plugin.start || entry.starting) return;
      entry.starting = true;
      try {
        await entry.plugin.start();
        this.failures.delete(entry.plugin.name);
        Events.emit("plugin.started", { name: entry.plugin.name });
        log3.info("started", { name: entry.plugin.name });
      } catch (e) {
        entry.enabled = false;
        this.failures.set(entry.plugin.name, e);
        await this.persistEnabled();
        log3.error(`start() failed for ${entry.plugin.name}`, e);
        Events.emit("plugin.error", {
          name: entry.plugin.name,
          phase: "start",
          error: e
        });
      } finally {
        entry.starting = false;
      }
    }
    async stopEntry(entry) {
      try {
        if (entry.plugin.stop) await entry.plugin.stop();
      } catch (e) {
        log3.error(`stop() failed for ${entry.plugin.name}`, e);
        Events.emit("plugin.error", {
          name: entry.plugin.name,
          phase: "stop",
          error: e
        });
      } finally {
        const removed = Patcher.unpatchAll(entry.plugin.name);
        if (removed > 0) log3.debug("removed leftover patches", { name: entry.plugin.name, removed });
        this.failures.delete(entry.plugin.name);
        entry.scope.dispose();
        Events.emit("plugin.stopped", { name: entry.plugin.name });
      }
    }
    async persistEnabled() {
      const enabled = [...this.entries.values()].filter((e) => e.enabled).map((e) => e.plugin.name);
      await Settings.set(SETTINGS_ENABLED, enabled);
    }
  };
  var Plugins = new PluginManager();

  // src/core/themes.ts
  var log4 = Logger.get("themes");
  var STYLE_ID = "monocord-theme";
  var CUSTOM_STYLE_ID = "monocord-custom-css";
  var SETTINGS_ENABLED2 = "theme.enabled";
  var SETTINGS_ACTIVE = "theme.active";
  var SETTINGS_CUSTOM_CSS = "theme.customCss";
  var SETTINGS_QUICK_CSS = "theme.quickCss";
  var SETTINGS_DEFAULT_APPLIED = "theme.defaultApplied";
  var DEFAULT_THEME = "system24";
  var bridge = null;
  function setThemeBridge(b) {
    bridge = b;
  }
  function ensureStyle(id) {
    let el2 = document.getElementById(id);
    if (!el2) {
      el2 = document.createElement("style");
      el2.id = id;
      el2.setAttribute("data-monocord", "true");
      (document.head ?? document.documentElement).appendChild(el2);
    }
    return el2;
  }
  function setCss(id, css) {
    const el2 = ensureStyle(id);
    if (!/^\s*@import/m.test(css) && "adoptedStyleSheets" in document) {
      let sheet = adoptedSheets.get(id);
      if (!sheet) {
        sheet = new CSSStyleSheet();
        adoptedSheets.set(id, sheet);
        document.adoptedStyleSheets = [
          ...document.adoptedStyleSheets.filter((s) => s !== sheet),
          sheet
        ];
      }
      try {
        sheet.replaceSync(css);
        el2.textContent = "";
        return;
      } catch {
      }
    }
    detachAdopted(id);
    el2.textContent = css;
  }
  function detachAdopted(id) {
    const sheet = adoptedSheets.get(id);
    if (!sheet) return;
    adoptedSheets.delete(id);
    document.adoptedStyleSheets = document.adoptedStyleSheets.filter(
      (s) => s !== sheet
    );
  }
  var adoptedSheets = /* @__PURE__ */ new Map();
  function splitImports(css) {
    const imports = [];
    const body = css.split("\n").filter((line) => {
      const t = line.trim();
      if (/^@import\b/.test(t)) {
        imports.push(t);
        return false;
      }
      return true;
    }).join("\n");
    return { imports, body };
  }
  var Themes = {
    /** Currently enabled theme names, in apply order. */
    enabled: [],
    /** QuickCSS (custom CSS) toggle — Vencord's `useQuickCss`. */
    quickCss: true,
    /** Custom CSS payload (cached for re-application). */
    customCss: "",
    /** Legacy compat: first enabled theme, if any. */
    get active() {
      return this.enabled[0] ?? null;
    },
    /** Apply raw CSS as the only enabled theme (no persistence). */
    applyCss(css, name = "custom") {
      this.enabled = [name];
      void this.reapply();
      Events.emit("theme.changed", { enabled: this.enabled });
    },
    /** Clear all enabled themes. */
    clear() {
      this.enabled = [];
      void this.reapply();
      Events.emit("theme.changed", { enabled: [] });
    },
    /** List themes known to the main process. */
    async list() {
      if (!bridge) return [];
      try {
        return await bridge.listThemes();
      } catch (e) {
        log4.error("theme list failed", e);
        return [];
      }
    },
    /** Toggle a theme in the enabled set; persists the choice. */
    async toggle(name) {
      const idx = this.enabled.indexOf(name);
      if (idx >= 0) this.enabled.splice(idx, 1);
      else this.enabled.push(name);
      this.enabled = [...this.enabled];
      await this.reapply();
      await Settings.set(SETTINGS_ENABLED2, this.enabled);
      Events.emit("theme.changed", { enabled: this.enabled });
    },
    /** Replace the enabled set with a single theme (legacy activate). */
    async activate(name) {
      this.enabled = name === null ? [] : [name];
      await this.reapply();
      await Settings.set(SETTINGS_ENABLED2, this.enabled);
      Events.emit("theme.changed", { enabled: this.enabled });
    },
    /** Set QuickCSS on/off (Vencord's useQuickCss setting). */
    async setQuickCssEnabled(on2) {
      this.quickCss = on2;
      setCss(CUSTOM_STYLE_ID, on2 ? this.customCss : "");
      await Settings.set(SETTINGS_QUICK_CSS, on2);
    },
    /** Set user custom CSS (QuickCSS — appended after enabled themes). */
    async setCustomCss(css) {
      this.customCss = css;
      setCss(CUSTOM_STYLE_ID, this.quickCss ? css : "");
      await Settings.set(SETTINGS_CUSTOM_CSS, css);
    },
    /** Fetch + concatenate all enabled theme CSS and apply it. */
    async reapply() {
      if (!bridge) return;
      const imports = [];
      const bodies = [];
      const seenImports = /* @__PURE__ */ new Set();
      for (const name of this.enabled) {
        try {
          const css = await bridge.getThemeCss(name);
          if (css === null) {
            log4.warn(`theme not found: ${name}`);
            continue;
          }
          const { imports: im, body } = splitImports(css);
          for (const i of im) {
            if (!seenImports.has(i)) {
              seenImports.add(i);
              imports.push(i);
            }
          }
          bodies.push(`/* --- theme: ${name} --- */
${body}`);
        } catch (e) {
          log4.error(`theme ${name} failed`, e);
        }
      }
      setCss(STYLE_ID, `${imports.join("\n")}
${bodies.join("\n")}`);
    },
    /** Re-apply persisted themes + QuickCSS (called on framework start). */
    async restore() {
      await Settings.ready();
      this.customCss = await Settings.get(SETTINGS_CUSTOM_CSS, "") ?? "";
      this.quickCss = await Settings.get(SETTINGS_QUICK_CSS, true) ?? true;
      setCss(CUSTOM_STYLE_ID, this.quickCss ? this.customCss : "");
      let enabled = await Settings.get(SETTINGS_ENABLED2, []) ?? [];
      if (enabled.length === 0) {
        const legacy = await Settings.get(SETTINGS_ACTIVE, null);
        if (legacy) enabled = [legacy];
      }
      if (!await Settings.get(SETTINGS_DEFAULT_APPLIED, false)) {
        await Settings.set(SETTINGS_DEFAULT_APPLIED, true);
        enabled = [DEFAULT_THEME];
      }
      this.enabled = enabled;
      await Settings.set(SETTINGS_ENABLED2, enabled);
      await this.reapply();
      Events.emit("theme.changed", { enabled });
    },
    /**
     * Inject CSS owned by a plugin under a unique id. Independent of the
     * enabled themes and QuickCSS — several plugins can coexist.
     */
    injectCss(id, css) {
      setCss(`monocord-plugin-${id}`, css);
    },
    /** Remove CSS previously injected via `injectCss`. */
    removeCss(id) {
      detachAdopted(`monocord-plugin-${id}`);
      document.getElementById(`monocord-plugin-${id}`)?.remove();
    }
  };

  // src/core/webpack/index.ts
  var log5 = Logger.get("webpack");
  function chunkArrays() {
    const out = [];
    for (const key of Object.keys(window)) {
      if (!key.startsWith("webpackChunk")) continue;
      const v = window[key];
      if (Array.isArray(v)) out.push(v);
    }
    return out;
  }
  var WebpackFinder = class {
    req = null;
    probeToken = 0;
    /** Memoized exports lookups: moduleId → exports | undefined */
    exportCache = /* @__PURE__ */ new Map();
    warnedNoChunk = false;
    factoryPatches = [];
    interceptedChunks = /* @__PURE__ */ new WeakSet();
    patchedFactories = /* @__PURE__ */ new WeakSet();
    /** Whether the webpack require function has been captured. */
    get ready() {
      return this.req !== null;
    }
    /**
     * Locate `__webpack_require__` by pushing a probe chunk. Idempotent.
     * Returns false when no chunk array exists yet (Discord still loading).
     */
    ensure() {
      if (this.req) return true;
      const chunks = chunkArrays();
      if (chunks.length === 0) {
        if (!this.warnedNoChunk) {
          this.warnedNoChunk = true;
          log5.debug("no webpackChunk* arrays on window yet");
        }
        return false;
      }
      for (const chunksArr of chunks) {
        const token = `__monocord_probe_${++this.probeToken}`;
        let captured = null;
        try {
          chunksArr.push?.([
            [token],
            {},
            (requireFn) => {
              captured = requireFn;
            }
          ]);
        } catch (e) {
          log5.debug("probe push threw", e);
          continue;
        }
        if (captured) {
          this.req = captured;
          log5.info("captured webpack require", { chunks: chunks.length });
          return true;
        }
      }
      return false;
    }
    /**
     * Register source patches for module factories.
     *
     * Discord/rspack registers lazy chunks by pushing onto `webpackChunk*`.
     * Intercepting that push lets us rewrite a factory before the runtime
     * executes it — necessary for code that is only reachable through a
     * closure. Patches installed after a factory ran cannot affect its
     * cached exports, so callers should register early (plugin start).
     *
     * Returns how many already-registered factories matched immediately.
     */
    patchFactories(...patches) {
      this.factoryPatches.push(...patches);
      const req = this.require_();
      this.interceptChunks();
      let patched = 0;
      for (const [id, factory] of Object.entries(req.m ?? {})) {
        if (typeof factory !== "function") continue;
        const replacement = this.applyFactoryPatches(id, factory);
        if (replacement) {
          req.m[id] = replacement;
          patched++;
        }
      }
      log5.info("factory patches installed", {
        patches: this.factoryPatches.length,
        alreadyRegistered: patched
      });
      return patched;
    }
    /** Wrap every chunk array's push so new factories can be rewritten. */
    interceptChunks() {
      if (this.factoryPatches.length === 0) return;
      for (const chunks of chunkArrays()) {
        if (this.interceptedChunks.has(chunks)) continue;
        this.interceptedChunks.add(chunks);
        const target = chunks;
        const originalPush = target.push;
        if (typeof originalPush !== "function") continue;
        const self = this;
        target.push = function patchedPush(...items) {
          self.transformChunkModules(items);
          return originalPush.apply(this, items);
        };
      }
    }
    /** Rewrite module factories in a `[chunkIds, modules, runtime]` tuple. */
    transformChunkModules(items) {
      for (const item of items) {
        if (!Array.isArray(item)) continue;
        const modules = item[1];
        if (modules === null || typeof modules !== "object") continue;
        const table = modules;
        for (const id of Object.keys(table)) {
          const factory = table[id];
          if (typeof factory !== "function") continue;
          const replacement = this.applyFactoryPatches(
            id,
            factory
          );
          if (replacement) table[id] = replacement;
        }
      }
    }
    /** Compile and return a patched factory, or null when none applied. */
    applyFactoryPatches(id, factory) {
      if (this.patchedFactories.has(factory)) return null;
      let source;
      try {
        source = Function.prototype.toString.call(factory);
      } catch {
        return null;
      }
      let next = source;
      const applied = [];
      for (const patch of this.factoryPatches) {
        if (!matchesFind(source, patch.find)) continue;
        patch.replacement.match.lastIndex = 0;
        const candidate = next.replace(
          patch.replacement.match,
          patch.replacement.replace
        );
        if (candidate === next) {
          log5.warn("factory patch had no effect", {
            moduleId: id,
            plugin: patch.plugin
          });
          continue;
        }
        next = candidate;
        applied.push(patch.plugin);
      }
      if (applied.length === 0) return null;
      try {
        const compiled = compileFactory(next);
        this.patchedFactories.add(compiled);
        log5.info("patched module factory", {
          moduleId: id,
          plugins: applied.join(", ")
        });
        return compiled;
      } catch (e) {
        log5.error("failed to compile patched module factory", {
          moduleId: id,
          error: e
        });
        return null;
      }
    }
    /** Poll until the require function is available or `timeoutMs` elapses. */
    async waitReady(timeoutMs = 3e4, intervalMs = 50) {
      const deadline = Date.now() + timeoutMs;
      while (Date.now() < deadline) {
        if (this.ensure()) return true;
        await new Promise((r) => setTimeout(r, intervalMs));
      }
      return this.ensure();
    }
    require_() {
      if (!this.req && !this.ensure()) {
        throw new ModuleNotFoundError("webpack runtime (no webpackChunk* arrays)");
      }
      return this.req;
    }
    /** All module ids currently known to webpack. */
    moduleIds() {
      const req = this.require_();
      return Object.keys(req.m ?? {});
    }
    /**
     * Get a module's exports. Uses the executed-module cache when present;
     * otherwise requires the module (executes its factory — side effects are
     * possible, same trade-off every client mod makes).
     */
    getExports(id) {
      if (this.exportCache.has(String(id))) return this.exportCache.get(String(id));
      const req = this.require_();
      let exports;
      const cached = req.c?.[id];
      if (cached && "exports" in cached) {
        exports = cached.exports;
      } else {
        try {
          exports = req(id);
        } catch (e) {
          log5.debug(`module ${String(id)} threw on require`, e);
          exports = void 0;
        }
      }
      this.exportCache.set(String(id), exports);
      return exports;
    }
    /** Iterate (id, exports) for every loadable module. */
    *iterModules() {
      for (const id of this.moduleIds()) {
        const exports = this.getExports(id);
        if (exports !== void 0) yield [id, exports];
      }
    }
    /** First module whose exports satisfy `predicate`. */
    find(predicate) {
      for (const [, exports] of this.iterModules()) {
        try {
          if (predicate(exports)) return exports;
        } catch {
        }
      }
      return void 0;
    }
    /** All modules whose exports satisfy `predicate`. */
    findAll(predicate) {
      const out = [];
      for (const [, exports] of this.iterModules()) {
        try {
          if (predicate(exports)) out.push(exports);
        } catch {
        }
      }
      return out;
    }
    // Webpack's module table is stable after boot, so successful lookups
    // are memoized — each uncached find scans and requires every module.
    // Misses are NOT cached: Discord lazy-loads chunks, so a module absent
    // now may appear later and a cached miss would hide it forever.
    findCache = /* @__PURE__ */ new Map();
    cached(key, compute) {
      if (this.findCache.has(key)) return this.findCache.get(key);
      const v = compute();
      if (v !== void 0) this.findCache.set(key, v);
      return v;
    }
    /**
     * Find a module whose exports (or `exports.default`) contain every
     * listed property.
     */
    findByProps(...props) {
      return this.cached(
        `props:${props.join("|")}`,
        () => this.find((exports) => hasProps(exports, props))
      );
    }
    /** Find all modules matching `findByProps`. */
    findAllByProps(...props) {
      const key = `allprops:${props.join("|")}`;
      return this.cached(
        key,
        () => this.findAll((exports) => hasProps(exports, props))
      ) ?? [];
    }
    /** Find a module by `displayName` on exports or `exports.default`. */
    findByDisplayName(name) {
      return this.cached(
        `display:${name}`,
        () => this.find(
          (exports) => displayNameOf(exports) === name || isObject(exports) && displayNameOf(exports.default) === name
        )
      );
    }
    /** Find a Flux store by display name. */
    findStore(name) {
      const mod = this.findByDisplayName(name);
      if (isObject(mod) && isObject(mod.default)) return mod.default;
      return mod;
    }
    /** Clear memoized exports (e.g. after a Discord reload). */
    invalidate() {
      this.exportCache.clear();
      this.findCache.clear();
    }
    /** Drop the captured require entirely — next ensure() re-probes. */
    reset() {
      this.req = null;
      this.exportCache.clear();
      this.findCache.clear();
      this.warnedNoChunk = false;
      this.factoryPatches = [];
    }
  };
  function matchesFind(source, find) {
    if (typeof find === "string") return source.includes(find);
    find.lastIndex = 0;
    return find.test(source);
  }
  function compileFactory(source) {
    const trimmed = source.trimStart();
    const body = trimmed.startsWith("(") ? trimmed : `function${trimmed.slice(trimmed.indexOf("("))}`;
    const expression = `0,${body}
//# sourceURL=monocord-module-factory.js`;
    return (0, eval)(expression);
  }
  function isObject(v) {
    return typeof v === "object" && v !== null;
  }
  function hasProps(exports, props) {
    const candidates = [exports];
    if (isObject(exports) && "default" in exports) candidates.push(exports.default);
    return candidates.some(
      (c) => isObject(c) && props.every((p) => p in c)
    );
  }
  function displayNameOf(v) {
    if (!isObject(v)) return void 0;
    const dn = v.displayName;
    return typeof dn === "string" ? dn : void 0;
  }
  var Webpack = new WebpackFinder();

  // src/core/api.ts
  function getReact() {
    return new Promise((resolve, reject) => {
      const candidates = [
        ["createElement", "useState", "useEffect", "createContext"],
        ["createElement", "useState", "useEffect"],
        ["createElement", "useEffect"],
        ["createElement", "Fragment", "useState"]
      ];
      const tryFind = () => {
        for (const props of candidates) {
          const react2 = Webpack.findByProps(...props);
          if (react2) {
            resolve(react2);
            return true;
          }
        }
        return false;
      };
      if (tryFind()) return;
      const timer2 = setInterval(() => {
        if (tryFind()) clearInterval(timer2);
      }, 250);
      setTimeout(() => {
        clearInterval(timer2);
        reject(new Error("could not locate React module"));
      }, 3e4);
    });
  }
  var Api = {
    Webpack,
    Patcher,
    Events,
    eventScope,
    Plugins,
    Settings,
    Themes,
    Logger,
    React: { get: getReact },
    version: "0.1.0"
  };

  // src/plugins/extras.ts
  var log6 = Logger.get("extras");
  function cssPlugin(name, description, css) {
    const styleId = name.toLowerCase();
    return {
      name,
      description,
      authors: ["Monocord"],
      version: "0.1.0",
      start() {
        Api.Themes.injectCss(styleId, css);
      },
      stop() {
        Api.Themes.removeCss(styleId);
      }
    };
  }
  var RoundAvatars = cssPlugin(
    "RoundAvatars",
    "Round server and group icons instead of rounded squares.",
    `
    /* RoundAvatars */
    nav [class*="guilds"] img,
    [class*="guilds"] [class*="avatar"] img,
    [class*="guilds"] [class*="icon"] img {
        border-radius: 50% !important;
    }
    `
  );
  var ThinScrollbars = cssPlugin(
    "ThinScrollbars",
    "Slimmer, more subtle scrollbars.",
    `
    /* ThinScrollbars */
    ::-webkit-scrollbar {
        width: 8px;
        height: 8px;
    }
    ::-webkit-scrollbar-thumb {
        background: var(--scrollbar-thin-thumb, rgba(255,255,255,.16));
        border-radius: 4px;
    }
    ::-webkit-scrollbar-thumb:hover {
        background: var(--scrollbar-thin-thumb-hover, rgba(255,255,255,.24));
    }
    ::-webkit-scrollbar-track,
    ::-webkit-scrollbar-corner {
        background: transparent;
    }
    `
  );
  var DisableAnimations = cssPlugin(
    "DisableAnimations",
    "Remove UI animations and transitions for a snappier feel.",
    `
    /* DisableAnimations */
    *,
    *::before,
    *::after {
        animation-duration: 0.01ms !important;
        animation-iteration-count: 1 !important;
        transition-duration: 0.01ms !important;
        scroll-behavior: auto !important;
    }
    `
  );
  var BiggerText = cssPlugin(
    "BiggerText",
    "Slightly larger UI text (scales rem-based layout).",
    `
    /* BiggerText */
    html {
        font-size: 17px !important;
    }
    `
  );
  var NoTypingIndicator = {
    name: "NoTypingIndicator",
    description: "Hide typing indicators in message lists.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const store = Api.Webpack.findStore("TypingStore") ?? Api.Webpack.findByProps("getTypingUsers", "isTyping");
      if (store && typeof store === "object") {
        const target = store;
        const patched = [];
        for (const key of Object.keys(target)) {
          const fn = target[key];
          if (typeof fn === "function" && /typing|isTyping|getUsers|getAll/i.test(key)) {
            try {
              Api.Patcher.instead(
                this.name,
                target,
                key,
                () => /^is/i.test(key) ? false : []
              );
              patched.push(key);
            } catch {
            }
          }
        }
        if (patched.length > 0) {
          log6.info("typing store patched", { methods: patched });
          return;
        }
      }
      for (const displayName of [
        "TypingDots",
        "TypingIndicator",
        "MessageTypingIndicator",
        "TypingIndicatorContainer"
      ]) {
        const component = Api.Webpack.findByDisplayName(displayName);
        const proto = component && typeof component === "function" ? component.prototype : void 0;
        if (proto && typeof proto.render === "function") {
          try {
            Api.Patcher.instead(this.name, proto, "render", () => null);
            log6.info("typing component hidden", { displayName });
            return;
          } catch {
          }
        }
      }
      log6.warn("no typing indicator target found \u2014 plugin inactive");
    }
  };
  var timer;
  var FakeTyping = {
    name: "FakeTyping",
    description: "Pretend to be typing in the current channel.",
    authors: ["Monocord"],
    version: "0.1.0",
    settings: { intervalSeconds: 9 },
    settingsSchema: {
      intervalSeconds: {
        type: "number",
        label: "Interval (seconds)",
        description: "How often to send a typing indicator while enabled."
      }
    },
    async start() {
      const starter = Api.Webpack.findByProps("startTyping") ?? Api.Webpack.findByProps("typingStart");
      const channelStore = Api.Webpack.findStore("SelectedChannelStore") ?? Api.Webpack.findStore("SelectedChannelStore") ?? Api.Webpack.findByProps("getCurrentlySelectedChannelId");
      if (!starter || typeof starter !== "object") {
        log6.warn("typing action module not found \u2014 plugin inactive");
        return;
      }
      const interval = (await Api.Settings.get(
        "plugins.settings.FakeTyping.intervalSeconds",
        9
      ) ?? 9) * 1e3;
      const tick = () => {
        try {
          const channelId = channelStore?.getChannelId?.();
          if (!channelId) return;
          for (const key of Object.keys(starter)) {
            const fn = starter[key];
            if (typeof fn === "function" && /typing/i.test(key)) {
              fn(channelId);
              return;
            }
          }
          const direct = starter.startTyping;
          if (typeof direct === "function") {
            direct(channelId);
          }
        } catch (e) {
          log6.debug("fake typing tick failed", e);
        }
      };
      tick();
      timer = setInterval(tick, interval);
    },
    stop() {
      if (timer) clearInterval(timer);
      timer = void 0;
    }
  };
  function registerExtras() {
    for (const plugin of [
      RoundAvatars,
      ThinScrollbars,
      DisableAnimations,
      BiggerText,
      NoTypingIndicator,
      FakeTyping
    ]) {
      try {
        Api.Plugins.register(plugin);
      } catch (e) {
        log6.error(`failed to register ${plugin.name}`, e);
      }
    }
  }

  // src/plugins/examples/index.ts
  var log7 = Logger.get("examples");
  function patchRender(pluginName, component, callback) {
    const candidates = [];
    if (typeof component === "function" && component.prototype) {
      candidates.push(component.prototype);
    }
    if (typeof component === "object" && component !== null) {
      candidates.push(component);
    }
    for (const target of candidates) {
      const render = target.render;
      if (typeof render === "function") {
        Api.Patcher.after(pluginName, target, "render", callback);
        return true;
      }
    }
    return false;
  }
  var StatusNote = {
    name: "StatusNote",
    description: "Example: show a custom note on user profile popouts.",
    authors: ["Monocord"],
    version: "0.1.0",
    settings: { note: "Monocord user" },
    async start() {
      const component = Api.Webpack.findByDisplayName("UserProfilePopout") ?? Api.Webpack.findByProps("UserProfilePopout");
      if (!component) {
        log7.warn("UserProfilePopout module not found \u2014 plugin inactive");
        return;
      }
      const react2 = await Api.React.get().catch(() => null);
      if (!react2) {
        log7.warn("React module not found \u2014 plugin inactive");
        return;
      }
      const createElement = react2.createElement;
      const isValidElement = react2.isValidElement;
      const cloneElement = react2.cloneElement;
      const note = await Api.Settings.get("plugins.settings.StatusNote.note") ?? "Monocord user";
      const ok = patchRender(this.name, component, (ret) => {
        if (!isValidElement(ret)) return ret;
        const el2 = ret;
        const children = Array.isArray(el2.props?.children) ? el2.props.children : [el2.props?.children];
        return cloneElement(
          ret,
          {},
          ...children.filter(Boolean),
          createElement(
            "div",
            {
              style: {
                padding: "8px 12px",
                fontSize: 12,
                color: "var(--text-muted, #949ba4)"
              }
            },
            `\u270E ${note}`
          )
        );
      });
      if (!ok) log7.warn("could not patch UserProfilePopout.render");
    }
  };
  var CompactMessagesCSS = `
    /* CompactMessages: tighter message rows */
    li[id^="chat-messages-"] > div {
        padding-top: 2px !important;
        padding-bottom: 2px !important;
    }
`;
  var CompactMessages = {
    name: "CompactMessages",
    description: "Example: compact message list spacing via injected CSS.",
    authors: ["Monocord"],
    version: "0.1.0",
    settings: {},
    async start() {
      const cur = await Api.Settings.get("theme.customCss", "") ?? "";
      const cleaned = cur.split(CompactMessagesCSS).join("").trim();
      const combined = cleaned ? `${cleaned}
${CompactMessagesCSS}` : CompactMessagesCSS;
      await Api.Themes.setCustomCss(combined);
      await Api.Settings.set(
        "plugins.settings.CompactMessages.css",
        CompactMessagesCSS
      );
    },
    async stop() {
      const cur = await Api.Settings.get("theme.customCss", "") ?? "";
      const stripped = cur.split(CompactMessagesCSS).join("").trim();
      await Api.Themes.setCustomCss(stripped);
      await Api.Settings.set(
        "plugins.settings.CompactMessages.css",
        ""
      );
    }
  };
  var keyHandler = null;
  var ThemeCycler = {
    name: "ThemeCycler",
    description: "Example: Ctrl+Shift+T cycles through installed themes.",
    authors: ["Monocord"],
    version: "0.1.0",
    settings: {},
    async start() {
      const handler = async (e) => {
        if (!(e.ctrlKey && e.shiftKey && e.key.toLowerCase() === "t")) return;
        e.preventDefault();
        const themes = await Api.Themes.list();
        const names = themes.map((t) => t.name);
        if (names.length === 0) return;
        const current = Api.Themes.active;
        const idx = current ? names.indexOf(current) : -1;
        const next = names[(idx + 1) % names.length];
        try {
          await Api.Themes.activate(next === current ? null : next);
        } catch (err) {
          log7.warn("theme activation failed", err);
        }
      };
      window.addEventListener("keydown", handler);
      keyHandler = handler;
    },
    stop() {
      if (keyHandler) window.removeEventListener("keydown", keyHandler);
      keyHandler = null;
    }
  };
  function registerExamples() {
    for (const plugin of [StatusNote, CompactMessages, ThemeCycler]) {
      try {
        Api.Plugins.register(plugin);
      } catch (e) {
        log7.error(`failed to register ${plugin.name}`, e);
      }
    }
  }

  // src/plugins/dom-utils.ts
  var rootWatches = /* @__PURE__ */ new WeakMap();
  function ensureRootWatch(root) {
    let watch = rootWatches.get(root);
    if (!watch) {
      const watchers = [];
      const scanNode = (node) => {
        for (const w of watchers) {
          if (node.matches(w.selector) && !w.seen.has(node)) {
            w.seen.add(node);
            w.cb(node);
          }
          node.querySelectorAll(w.selector).forEach((el2) => {
            if (!w.seen.has(el2)) {
              w.seen.add(el2);
              w.cb(el2);
            }
          });
        }
      };
      const observer = new MutationObserver((mutations) => {
        for (const m of mutations) {
          for (const node of m.addedNodes) {
            if (node instanceof Element) scanNode(node);
          }
        }
      });
      observer.observe(root, { childList: true, subtree: true });
      watch = { observer, watchers };
      rootWatches.set(root, watch);
    }
    return watch;
  }
  function observeAdded(selector, cb, root = document.body) {
    const watch = ensureRootWatch(root);
    const watcher = { selector, cb, seen: /* @__PURE__ */ new WeakSet() };
    root.querySelectorAll(selector).forEach((el2) => {
      watcher.seen.add(el2);
      cb(el2);
    });
    watch.watchers.push(watcher);
    return () => {
      const i = watch.watchers.indexOf(watcher);
      if (i >= 0) watch.watchers.splice(i, 1);
      if (watch.watchers.length === 0) {
        watch.observer.disconnect();
        rootWatches.delete(root);
      }
    };
  }
  function on(target, type, selector, handler, capture = false) {
    const listener = (ev) => {
      const t = ev.target;
      if (!(t instanceof Element)) return;
      const el2 = selector ? t.closest(selector) : t;
      if (el2) handler(el2, ev);
    };
    target.addEventListener(type, listener, capture);
    return () => target.removeEventListener(type, listener, capture);
  }
  function el(tag, style, ...children) {
    const node = document.createElement(tag);
    if (style) Object.assign(node.style, style);
    for (const c of children) {
      if (c == null) continue;
      node.append(c instanceof Node ? c : document.createTextNode(c));
    }
    return node;
  }
  function contextMenu(x, y, items) {
    const menu = el(
      "div",
      {
        position: "fixed",
        left: `${x}px`,
        top: `${y}px`,
        zIndex: "100001",
        background: "var(--background-floating, #111214)",
        border: "1px solid var(--background-modifier-accent, #3f4147)",
        borderRadius: "8px",
        padding: "4px",
        minWidth: "180px",
        boxShadow: "0 8px 24px rgba(0,0,0,.4)"
      }
    );
    const close = () => menu.remove();
    for (const item of items) {
      const b = el(
        "button",
        {
          display: "block",
          width: "100%",
          textAlign: "left",
          background: "none",
          border: "none",
          color: item.danger ? "var(--status-danger, #f23f43)" : "var(--text-normal, #dbdee1)",
          padding: "8px 12px",
          fontSize: "13px",
          cursor: "pointer",
          borderRadius: "4px"
        },
        item.label
      );
      b.addEventListener("mouseenter", () => b.style.background = "var(--background-modifier-hover, rgba(78,80,88,.3))");
      b.addEventListener("mouseleave", () => b.style.background = "none");
      b.addEventListener("click", () => {
        close();
        item.action();
      });
      menu.append(b);
    }
    document.body.append(menu);
    setTimeout(() => {
      document.addEventListener("click", close, { once: true });
      document.addEventListener("contextmenu", close, { once: true });
      document.addEventListener("keydown", function onKey(e) {
        if (e.key === "Escape") {
          close();
          document.removeEventListener("keydown", onKey);
        }
      });
    }, 0);
    return close;
  }
  function toast(text, ms = 2200) {
    const box = el(
      "div",
      {
        position: "fixed",
        top: "64px",
        left: "50%",
        transform: "translateX(-50%)",
        zIndex: "100000",
        background: "var(--background-floating, #111214)",
        color: "var(--text-normal, #dbdee1)",
        border: "1px solid var(--background-modifier-accent, #3f4147)",
        borderRadius: "8px",
        padding: "10px 18px",
        fontSize: "14px",
        boxShadow: "0 8px 24px rgba(0,0,0,.4)",
        pointerEvents: "none"
      },
      text
    );
    document.body.append(box);
    setTimeout(() => box.remove(), ms);
  }

  // src/plugins/mods.ts
  var log8 = Logger.get("mods");
  function bridge2() {
    return window.monocord ?? null;
  }
  function isObj(v) {
    return typeof v === "object" && v !== null;
  }
  function findDispatcher() {
    const looksLike = (o) => {
      if (!isObj(o)) return false;
      const d = o;
      return typeof d.subscribe === "function" && (typeof d.dispatch === "function" || "_actionHandlers" in o || "_subscriptions" in o || "_actionHandlerSubscribers" in o);
    };
    const found = Api.Webpack.findAll((exp) => {
      if (looksLike(exp)) return true;
      if (isObj(exp) && looksLike(exp.default)) return true;
      return false;
    });
    for (const exp of found) {
      const direct = looksLike(exp) ? exp : void 0;
      const viaDefault = isObj(exp) ? exp.default : void 0;
      const d = direct ?? (looksLike(viaDefault) ? viaDefault : void 0);
      if (d) return d;
    }
    return null;
  }
  function findStoreByName(name) {
    const store = Api.Webpack.findStore(name);
    return isObj(store) ? store : null;
  }
  var MessageLogger = {
    name: "MessageLogger",
    description: "Logs edited and deleted messages to a local file.",
    authors: ["Monocord"],
    version: "0.1.0",
    settings: { includeContent: true },
    settingsSchema: {
      includeContent: {
        type: "boolean",
        label: "Include message content",
        description: "Store message text in the log (otherwise only metadata)."
      }
    },
    async start() {
      const dispatcher2 = findDispatcher();
      if (!dispatcher2) {
        log8.warn("flux dispatcher not found \u2014 plugin inactive");
        return;
      }
      const cache = /* @__PURE__ */ new Map();
      const CACHE_MAX = 1e4;
      const b = bridge2();
      if (!b) {
        log8.warn("no bridge \u2014 cannot persist logs");
        return;
      }
      const includeContent = (await Api.Settings.get(
        "plugins.settings.MessageLogger.includeContent",
        true
      ) ?? true) === true;
      const remember = (channelId, msg) => {
        const id = String(msg.id ?? "");
        if (!id) return;
        const key = `${channelId}:${id}`;
        cache.set(key, msg);
        if (cache.size > CACHE_MAX) {
          const first = cache.keys().next().value;
          if (first !== void 0) cache.delete(first);
        }
      };
      const summarize = (msg) => {
        if (!msg) return null;
        const author = isObj(msg.author) ? msg.author : {};
        return {
          messageId: msg.id,
          authorId: author.id,
          author: [author.username, author.discriminator].filter(Boolean).join("#") || author.globalName || author.username,
          content: includeContent ? msg.content : void 0,
          timestamp: msg.timestamp,
          attachments: Array.isArray(msg.attachments) ? msg.attachments.map(
            (a) => a.filename
          ) : void 0
        };
      };
      const pending = [];
      const flush = () => {
        if (pending.length === 0) return;
        const lines = pending.splice(0, pending.length);
        void b.invoke("msglog", lines).catch(() => {
        });
      };
      const timer2 = setInterval(flush, 3e3);
      const onHide = () => flush();
      window.addEventListener("pagehide", onHide);
      this.__teardown = () => {
        clearInterval(timer2);
        window.removeEventListener("pagehide", onHide);
        flush();
      };
      const write = (record) => {
        pending.push(JSON.stringify({ ts: Date.now(), ...record }));
        if (pending.length >= 100) flush();
      };
      dispatcher2.subscribe((action) => {
        const type = action.type;
        try {
          if (type === "MESSAGE_CREATE") {
            const channelId = String(action.channelId ?? "");
            const msg = action.message;
            if (channelId && isObj(msg)) remember(channelId, msg);
            return;
          }
          if (type === "MESSAGE_UPDATE") {
            const channelId = String(action.channelId ?? "");
            const msg = action.message;
            if (!channelId || !isObj(msg)) return;
            const id = String(msg.id ?? "");
            const prev = cache.get(`${channelId}:${id}`);
            const edited = msg.edited_timestamp ?? msg.editedTimestamp ?? null;
            if (prev && edited && prev.content !== msg.content) {
              write({
                kind: "edit",
                channelId,
                before: summarize(prev),
                after: summarize(msg)
              });
            }
            remember(channelId, msg);
            return;
          }
          if (type === "MESSAGE_DELETE") {
            const channelId = String(action.channelId ?? "");
            const mid = isObj(action.message) ? action.message.id : null;
            const id = String(action.id ?? mid ?? "");
            const msg = isObj(action.message) ? action.message : cache.get(`${channelId}:${id}`);
            write({ kind: "delete", channelId, message: summarize(msg) });
            cache.delete(`${channelId}:${id}`);
            return;
          }
          if (type === "MESSAGE_DELETE_BULK") {
            const channelId = String(action.channelId ?? "");
            const ids = Array.isArray(action.ids) ? action.ids : [];
            for (const id of ids) {
              const msg = cache.get(`${channelId}:${String(id)}`);
              write({
                kind: "delete",
                channelId,
                message: summarize(msg)
              });
              cache.delete(`${channelId}:${String(id)}`);
            }
          }
        } catch (e) {
          log8.debug("message logger handler failed", e);
        }
      });
      log8.info("message logger subscribed");
    },
    stop() {
      this.__teardown?.();
    }
  };
  var NoBlockedMessages = {
    name: "NoBlockedMessages",
    description: "Hide messages from users you have blocked or ignored.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const rel = findStoreByName("RelationshipStore");
      if (!rel) {
        log8.warn("RelationshipStore not found \u2014 plugin inactive");
        return;
      }
      const isBlocked = (id) => {
        if (typeof id !== "string" && typeof id !== "number") return false;
        for (const fn of ["isBlocked", "isIgnored", "isBlockedOrIgnored"]) {
          const f = rel[fn];
          if (typeof f === "function") {
            try {
              if (f.call(rel, id)) {
                return true;
              }
            } catch {
            }
          }
        }
        return false;
      };
      let patched = 0;
      for (const exp of Api.Webpack.findAll(() => true)) {
        const candidates = [exp];
        if (isObj(exp) && "default" in exp) candidates.push(exp.default);
        for (const c of candidates) {
          if (!isObj(c)) continue;
          if (c.displayName !== "Message") continue;
          const proto = c.prototype;
          if (!proto || typeof proto.render !== "function") continue;
          try {
            Api.Patcher.instead(
              this.name,
              proto,
              "render",
              function() {
                const props = this.props ?? {};
                const msg = isObj(props.message) ? props.message : {};
                const author = isObj(msg.author) ? msg.author : {};
                if (isBlocked(author.id)) return null;
                return void 0;
              }
            );
            patched += 1;
          } catch {
          }
        }
      }
      if (patched === 0) {
        log8.warn("no Message component found \u2014 plugin inactive");
      } else {
        log8.info("message render patched", { count: patched });
      }
    }
  };
  var cachedRules = [];
  var rulesLoaded = false;
  async function refreshRules() {
    const raw = await Api.Settings.get(
      "plugins.settings.TextReplace.rules"
    );
    if (Array.isArray(raw)) {
      cachedRules = raw.filter(isObj).map((r) => ({
        find: String(r.find ?? ""),
        replace: String(r.replace ?? "")
      })).filter((r) => r.find.length > 0);
    } else {
      cachedRules = [];
    }
    rulesLoaded = true;
  }
  function applyRules(content) {
    if (!rulesLoaded) void refreshRules();
    let out = content;
    for (const r of cachedRules) {
      out = out.split(r.find).join(r.replace);
    }
    return out;
  }
  function findSendModule() {
    const candidates = Api.Webpack.findAllByProps("sendMessage");
    for (const exp of candidates) {
      const targets = [exp];
      if (isObj(exp) && "default" in exp) targets.push(exp.default);
      for (const t of targets) {
        if (isObj(t) && typeof t.sendMessage === "function" && typeof t.editMessage === "function") {
          return t;
        }
      }
    }
    return null;
  }
  var TextReplace = {
    name: "TextReplace",
    description: "Automatically replace configured text in outgoing messages.",
    authors: ["Monocord"],
    version: "0.1.0",
    settings: { rules: [] },
    async start() {
      await refreshRules();
      Api.Events.on("settings.changed", (payload) => {
        const path = payload?.path;
        if (path === "plugins.settings.TextReplace.rules") {
          void refreshRules();
        }
      });
      const mod = findSendModule();
      if (!mod) {
        log8.warn("send module not found \u2014 plugin inactive");
        return;
      }
      Api.Patcher.before(this.name, mod, "sendMessage", (...args) => {
        for (const arg of args) {
          if (isObj(arg) && typeof arg.content === "string") {
            const next = applyRules(arg.content);
            if (next !== arg.content) arg.content = next;
          }
        }
        return args;
      });
      log8.info("send pipeline patched");
    }
  };
  var SILENT_FLAG = 4096;
  var SilentMessageToggle = {
    name: "SilentMessageToggle",
    description: "Ctrl+Alt+S toggles sending messages silently.",
    authors: ["Monocord"],
    version: "0.1.0",
    async start() {
      const mod = findSendModule();
      if (!mod) {
        log8.warn("send module not found \u2014 plugin inactive");
        return;
      }
      let silent = (await Api.Settings.get(
        "plugins.settings.SilentMessageToggle.active",
        false
      ) ?? false) === true;
      Api.Patcher.before(this.name, mod, "sendMessage", (...args) => {
        if (!silent) return args;
        for (const arg of args) {
          if (isObj(arg)) {
            const flags = typeof arg.flags === "number" ? arg.flags : 0;
            arg.flags = flags | SILENT_FLAG;
          }
        }
        return args;
      });
      const onKey = (e) => {
        if (e.ctrlKey && e.altKey && e.code === "KeyS") {
          e.preventDefault();
          e.stopPropagation();
          silent = !silent;
          void Api.Settings.set(
            "plugins.settings.SilentMessageToggle.active",
            silent
          );
          toast(silent ? "Silent messages ON" : "Silent messages OFF");
        }
      };
      document.addEventListener("keydown", onKey, true);
      this.__cleanup = () => {
        document.removeEventListener("keydown", onKey, true);
      };
      log8.info("silent toggle ready", { initial: silent });
    },
    stop() {
      this.__cleanup?.();
    }
  };
  var Translate = {
    name: "Translate",
    description: "Right-click a message to translate it.",
    authors: ["Monocord"],
    version: "0.1.0",
    settings: { target: "en" },
    settingsSchema: {
      target: {
        type: "select",
        label: "Target language",
        options: [
          ["en", "English"],
          ["es", "Spanish"],
          ["fr", "French"],
          ["de", "German"],
          ["pt", "Portuguese"],
          ["it", "Italian"],
          ["ja", "Japanese"],
          ["ko", "Korean"],
          ["zh-CN", "Chinese (Simplified)"],
          ["zh-TW", "Chinese (Traditional)"],
          ["ru", "Russian"],
          ["ar", "Arabic"],
          ["hi", "Hindi"],
          ["nl", "Dutch"],
          ["pl", "Polish"],
          ["tr", "Turkish"],
          ["vi", "Vietnamese"]
        ].map(([value, label]) => ({ value, label }))
      }
    },
    start() {
      const off = on(
        document,
        "contextmenu",
        '[id^="chat-messages-"]',
        (node, ev) => {
          const li = node;
          const m = /^chat-messages-(\d+)-(\d+)$/.exec(li.id);
          if (!m) return;
          const [, channelId = "", messageId = ""] = m;
          const store = findStoreByName("MessageStore");
          const getMessage2 = store?.getMessage;
          const msg = typeof getMessage2 === "function" ? getMessage2.call(
            store,
            channelId,
            messageId
          ) : null;
          const content = isObj(msg) ? msg.content : null;
          if (typeof content !== "string" || !content.trim()) return;
          ev.preventDefault();
          const me = ev.clientX + 8;
          const my = ev.clientY + 8;
          void translateAndShow(content, me, my);
        }
      );
      this.__off = off;
    },
    stop() {
      this.__off?.();
    }
  };
  async function translateAndShow(text, x, y) {
    const target = await Api.Settings.get("plugins.settings.Translate.target") ?? "en";
    const tip = el(
      "div",
      {
        position: "fixed",
        left: `${x}px`,
        top: `${y}px`,
        zIndex: "100001",
        maxWidth: "420px",
        background: "var(--background-floating, #111214)",
        color: "var(--text-normal, #dbdee1)",
        border: "1px solid var(--background-modifier-accent, #3f4147)",
        borderRadius: "8px",
        padding: "10px 14px",
        fontSize: "13px",
        lineHeight: "18px",
        boxShadow: "0 8px 24px rgba(0,0,0,.4)",
        whiteSpace: "pre-wrap"
      },
      "Translating\u2026"
    );
    document.body.append(tip);
    const close = () => tip.remove();
    document.addEventListener("click", close, { once: true });
    document.addEventListener("keydown", function onKey(e) {
      if (e.key === "Escape") {
        close();
        document.removeEventListener("keydown", onKey);
      }
    });
    try {
      const url = `https://translate.googleapis.com/translate_a/single?client=gtx&sl=auto&tl=${encodeURIComponent(target)}&dt=t&q=${encodeURIComponent(text.slice(0, 4e3))}`;
      const res = await fetch(url);
      const data = await res.json();
      const sentences = isObj(data) && Array.isArray(data[0]) ? data[0] : [];
      const translated = sentences.map((s) => Array.isArray(s) && typeof s[0] === "string" ? s[0] : "").join("");
      tip.textContent = translated || "(no translation)";
    } catch (e) {
      log8.warn("translate failed", e);
      tip.textContent = "Translation failed";
    }
  }
  var VcNarrator = {
    name: "VcNarrator",
    description: "Announces voice channel joins and leaves.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      if (typeof speechSynthesis === "undefined") {
        log8.warn("speechSynthesis unavailable \u2014 plugin inactive");
        return;
      }
      const dispatcher2 = findDispatcher();
      if (!dispatcher2) {
        log8.warn("flux dispatcher not found \u2014 plugin inactive");
        return;
      }
      const speak = (text) => {
        try {
          const u = new SpeechSynthesisUtterance(text);
          u.rate = 1.05;
          speechSynthesis.speak(u);
        } catch {
        }
      };
      dispatcher2.subscribe((action) => {
        try {
          if (action.type === "VOICE_CHANNEL_SELECT") {
            speak(action.channelId ? "Joined voice channel" : "Left voice channel");
          } else if (action.type === "CALL_DISCONNECT") {
            speak("Call ended");
          }
        } catch (e) {
          log8.debug("narrator failed", e);
        }
      });
      log8.info("voice narrator subscribed");
    }
  };
  var AutoDNDWhilePlaying = {
    name: "AutoDNDWhilePlaying",
    description: "Automatically go DND while a game is running.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const currentUser = findStoreByName("CurrentUserStore");
      const getUser = currentUser?.getCurrentUser;
      const hasGame = () => {
        try {
          let activities;
          const cu = typeof getUser === "function" ? getUser.call(
            currentUser
          ) : null;
          if (cu && Array.isArray(cu.activities)) {
            activities = cu.activities;
          } else if (typeof currentUser?.getActivities === "function") {
            activities = currentUser.getActivities.call(currentUser);
          }
          if (!Array.isArray(activities)) return false;
          return activities.some((a) => {
            if (!isObj(a)) return false;
            return a.type === 0 || a.type === 5;
          });
        } catch {
          return false;
        }
      };
      const setStatus = findStatusSetter();
      if (!setStatus) {
        log8.warn("status setter not found \u2014 plugin inactive");
        return;
      }
      let applied = false;
      const tick = () => {
        try {
          const playing = hasGame();
          if (playing && !applied) {
            setStatus("dnd");
            applied = true;
          } else if (!playing && applied) {
            setStatus("online");
            applied = false;
          }
        } catch (e) {
          log8.debug("auto dnd tick failed", e);
        }
      };
      tick();
      const timer2 = setInterval(tick, 15e3);
      this.__timer = timer2;
    },
    stop() {
      const t = this.__timer;
      if (t) clearInterval(t);
    }
  };
  function findStatusSetter() {
    const names = ["setStatus", "updateStatus", "saveStatus", "setStatusText"];
    for (const exp of Api.Webpack.findAll(() => true)) {
      const targets = [exp];
      if (isObj(exp) && "default" in exp) targets.push(exp.default);
      for (const t of targets) {
        if (!isObj(t)) continue;
        for (const name of names) {
          const f = t[name];
          if (typeof f === "function") {
            return (status) => {
              try {
                f.call(t, status);
              } catch (e) {
                log8.debug(`status setter ${name} failed`, e);
              }
            };
          }
        }
      }
    }
    return null;
  }
  var NoTrack = {
    name: "NoTrack",
    description: "Blocks Discord telemetry and analytics requests.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      void bridge2()?.invoke("track-block", true).catch((e) => log8.warn("track-block failed", e));
    },
    stop() {
      void bridge2()?.invoke("track-block", false).catch(() => {
      });
    }
  };
  var NoSystemBadge = {
    name: "NoSystemBadge",
    description: "Removes the unread-count badge on the taskbar icon.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      void bridge2()?.invoke("badge-block", true).catch((e) => log8.warn("badge-block failed", e));
    },
    stop() {
      void bridge2()?.invoke("badge-block", false).catch(() => {
      });
    }
  };
  function registerMods() {
    for (const plugin of [
      MessageLogger,
      NoBlockedMessages,
      TextReplace,
      SilentMessageToggle,
      Translate,
      VcNarrator,
      AutoDNDWhilePlaying,
      NoTrack,
      NoSystemBadge
    ]) {
      try {
        Api.Plugins.register(plugin);
      } catch (e) {
        log8.error(`failed to register ${plugin.name}`, e);
      }
    }
  }

  // src/plugins/custom-rpc.ts
  var log9 = Logger.get("custom-rpc");
  function bridge3() {
    return window.monocord ?? null;
  }
  var KEY = "plugins.settings.CustomRPC.";
  var k = (key) => `${KEY}${key}`;
  var ACTIVITY_TYPES = [
    { label: "Playing", value: "0" },
    { label: "Streaming", value: "1" },
    { label: "Listening", value: "2" },
    { label: "Watching", value: "3" },
    { label: "Competing", value: "6" }
  ];
  var TIMESTAMP_MODES = [
    { label: "None", value: "none" },
    { label: "Now (elapsed)", value: "now" },
    { label: "Time since midnight", value: "time" },
    { label: "Custom start/end", value: "custom" }
  ];
  var ASSET_PREFIXES = /^(?:gif|mp4|youtube|twitch|spotify|external|url):/i;
  function assetValue(raw) {
    const v = raw.trim();
    if (!v || ASSET_PREFIXES.test(v)) return v;
    if (/^https?:\/\//i.test(v) && (/\.gif($|\?)/i.test(v) || /media\.tenor\.com|i\.giphy\.com/i.test(v))) {
      return `gif:${v}`;
    }
    return v;
  }
  function previewAssetUrl(v) {
    return v.replace(/^(?:gif|mp4|youtube|twitch|spotify|external|url):/i, "");
  }
  var FIELDS = [
    "enabled",
    "appId",
    "appName",
    "details",
    "detailsUrl",
    "state",
    "stateUrl",
    "type",
    "streamUrl",
    "timestampMode",
    "startTime",
    "endTime",
    "imageBig",
    "imageBigUrl",
    "imageBigTooltip",
    "imageSmall",
    "imageSmallUrl",
    "imageSmallTooltip",
    "buttonOneText",
    "buttonOneUrl",
    "buttonTwoText",
    "buttonTwoUrl",
    "partySize",
    "partyMaxSize"
  ];
  async function readSettings() {
    const out = {};
    for (const key of FIELDS) {
      out[key] = await Api.Settings.get(k(key));
    }
    return out;
  }
  function buildActivity(s) {
    const appName = String(s.appName ?? "").trim();
    const appId = String(s.appId ?? "").trim();
    if (!appName && !appId) return null;
    const activity = {
      application_id: appId || "0",
      name: appName || "Custom RPC",
      type: Number(s.type ?? 0),
      flags: 1
    };
    const str = (key) => {
      const v = String(s[key] ?? "").trim();
      return v || void 0;
    };
    const details = str("details");
    const state = str("state");
    const detailsUrl = str("detailsUrl");
    const stateUrl = str("stateUrl");
    if (details) activity.details = details;
    if (state) activity.state = state;
    if (detailsUrl) activity.details_url = detailsUrl;
    if (stateUrl) activity.state_url = stateUrl;
    const type = Number(s.type ?? 0);
    if (type === 1) {
      const url = str("streamUrl");
      if (url) activity.url = url;
    }
    switch (String(s.timestampMode ?? "none")) {
      case "now":
        activity.timestamps = { start: Date.now() };
        break;
      case "time": {
        const d = /* @__PURE__ */ new Date();
        activity.timestamps = {
          start: Date.now() - (d.getHours() * 3600 + d.getMinutes() * 60 + d.getSeconds()) * 1e3
        };
        break;
      }
      case "custom": {
        const start = Number(s.startTime);
        const end = Number(s.endTime);
        const ts = {};
        if (start) ts.start = start;
        if (end) ts.end = end;
        if (Object.keys(ts).length) activity.timestamps = ts;
        break;
      }
    }
    const btn1 = str("buttonOneText");
    const btn1Url = str("buttonOneUrl");
    const btn2 = str("buttonTwoText");
    const btn2Url = str("buttonTwoUrl");
    if (btn1) {
      activity.buttons = [btn1, btn2].filter((b) => typeof b === "string" && b);
      activity.metadata = {
        button_urls: [btn1Url, btn2Url].filter((b) => typeof b === "string" && b)
      };
    }
    const imageBig = str("imageBig");
    const imageSmall = str("imageSmall");
    if (imageBig || imageSmall) {
      const assets = {};
      if (imageBig) assets.large_image = assetValue(imageBig);
      const bigTip = str("imageBigTooltip");
      const bigUrl = str("imageBigUrl");
      if (bigTip) assets.large_text = bigTip;
      if (bigUrl) assets.large_url = bigUrl;
      if (imageSmall) assets.small_image = assetValue(imageSmall);
      const smallTip = str("imageSmallTooltip");
      const smallUrl = str("imageSmallUrl");
      if (smallTip) assets.small_text = smallTip;
      if (smallUrl) assets.small_url = smallUrl;
      activity.assets = assets;
    }
    const partySize = Number(s.partySize);
    const partyMax = Number(s.partyMaxSize);
    if (partySize > 0 && partyMax > 0) {
      activity.party = { size: [partySize, partyMax] };
    }
    return activity;
  }
  function toIpcActivity(activity) {
    const out = { type: activity.type ?? 0 };
    for (const key of ["name", "state", "details", "url", "timestamps", "party", "assets"]) {
      if (activity[key] != null) out[key] = activity[key];
    }
    const assets = activity.assets;
    if (assets) {
      const clean = {};
      for (const key of ["large_image", "large_text", "small_image", "small_text"]) {
        const v = assets[key];
        if (typeof v === "string" && v) {
          clean[key] = previewAssetUrl(v);
        }
      }
      out.assets = clean;
    }
    return out;
  }
  var TYPE_LABELS = ["Playing", "Streaming", "Listening", "Watching", "", "", "Competing"];
  function imageOrKey(h2, key, appId, size) {
    const style = {
      width: size,
      height: size,
      borderRadius: size > 40 ? 8 : "50%",
      objectFit: "cover",
      flexShrink: 0
    };
    const url = previewAssetUrl(key);
    if (/^https?:\/\//.test(url)) {
      return h2("img", { src: url, style, alt: "" });
    }
    if (!key && appId) {
      return h2("img", {
        src: `https://cdn.discordapp.com/app-icons/${appId}/${appId}.png?size=128`,
        style,
        alt: "",
        onError: (e) => {
          if (e.target?.style) e.target.style.display = "none";
        }
      });
    }
    return h2(
      "div",
      {
        style: {
          ...style,
          background: "var(--background-modifier-accent, #3f4147)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          fontSize: size > 40 ? 22 : 11,
          fontWeight: 700,
          color: "var(--text-muted, #949ba4)",
          overflow: "hidden",
          textOverflow: "ellipsis",
          padding: 4,
          boxSizing: "border-box"
        }
      },
      key ? key.slice(0, 2).toUpperCase() : "?"
    );
  }
  function presencePreview(h2, ctx, str, num, type, tsMode) {
    const muted = "var(--text-muted, #949ba4)";
    const normal = "var(--text-normal, #dbdee1)";
    const appName = str("appName").trim();
    const appId = str("appId").trim();
    const details = str("details").trim();
    const state = str("state").trim();
    const detailsUrl = str("detailsUrl").trim();
    const stateUrl = str("stateUrl").trim();
    const imageBig = str("imageBig").trim();
    const imageSmall = str("imageSmall").trim();
    const btn1 = str("buttonOneText").trim();
    const btn2 = str("buttonTwoText").trim();
    const partySize = num("partySize");
    const partyMax = num("partyMaxSize");
    const configured = ctx.enabled && val0(ctx, "enabled") !== false && (appName || appId);
    if (!configured) {
      return h2(
        "div",
        {
          style: {
            background: "var(--background-secondary, #2b2d31)",
            borderRadius: 8,
            padding: "18px 14px",
            fontSize: 13,
            color: muted,
            textAlign: "center"
          }
        },
        ctx.enabled ? "No presence configured \u2014 set an app name or ID below." : "Plugin is disabled \u2014 enable it to see a preview."
      );
    }
    const linkStyle = (url) => ({
      color: url ? "var(--brand-500, #5865f2)" : normal,
      textDecoration: url ? "underline" : "none",
      cursor: url ? "pointer" : "default"
    });
    const line = (text, url, size = 13) => text ? h2(
      "div",
      { style: { fontSize: size, marginTop: 2, ...linkStyle(url) } },
      url ? h2("a", { href: url, style: { color: "inherit" } }, text) : text
    ) : null;
    const buttons = [btn1, btn2].filter(Boolean);
    const tsText = tsMode === "now" ? "Elapsed: live" : tsMode === "time" ? "Since midnight" : tsMode === "custom" ? [str("startTime"), str("endTime")].some((v) => Number(v) > 0) ? "Custom range" : "Custom (unset)" : null;
    return h2(
      "div",
      {
        style: {
          background: "var(--background-secondary, #2b2d31)",
          borderRadius: 8,
          padding: 12,
          display: "flex",
          gap: 12,
          maxWidth: 460
        }
      },
      // images
      h2(
        "div",
        { style: { position: "relative", flexShrink: 0 } },
        imageOrKey(h2, imageBig, appId, 68),
        imageSmall ? h2("div", {
          style: {
            position: "absolute",
            right: -6,
            bottom: -6,
            borderRadius: "50%",
            border: "3px solid var(--background-secondary, #2b2d31)"
          }
        }, imageOrKey(h2, imageSmall, appId, 30)) : null
      ),
      // text column
      h2(
        "div",
        { style: { flex: 1, minWidth: 0 } },
        h2(
          "div",
          { style: { display: "flex", alignItems: "baseline", gap: 6 } },
          h2(
            "div",
            {
              style: {
                fontSize: 14,
                fontWeight: 600,
                color: normal,
                overflow: "hidden",
                textOverflow: "ellipsis",
                whiteSpace: "nowrap"
              }
            },
            appName || "Custom RPC"
          ),
          h2(
            "div",
            { style: { fontSize: 11, color: muted, flexShrink: 0 } },
            TYPE_LABELS[type] ?? "Playing"
          )
        ),
        line(details, detailsUrl),
        line(state, stateUrl),
        buttons.length ? h2(
          "div",
          { style: { display: "flex", gap: 6, marginTop: 8 } },
          buttons.map(
            (b, i) => h2(
              "div",
              {
                key: i,
                style: {
                  background: "var(--button-secondary-background, #4e5058)",
                  color: normal,
                  borderRadius: 4,
                  padding: "4px 10px",
                  fontSize: 12,
                  fontWeight: 500
                }
              },
              b
            )
          )
        ) : null,
        h2(
          "div",
          {
            style: {
              display: "flex",
              gap: 10,
              marginTop: 6,
              fontSize: 11,
              color: muted
            }
          },
          tsText ? h2("span", null, tsText) : null,
          partySize > 0 && partyMax > 0 ? h2("span", null, `${partySize}/${partyMax} in party`) : null
        )
      )
    );
  }
  function val0(ctx, key) {
    return ctx.values[key] ?? CustomRPC.settings?.[key];
  }
  var dispatcher;
  async function pushConfig() {
    const s = await readSettings();
    const enabled = s.enabled !== false;
    const activity = enabled ? buildActivity(s) : null;
    if (dispatcher === void 0) dispatcher = findDispatcher();
    if (dispatcher?.dispatch) {
      dispatcher.dispatch({
        type: "LOCAL_ACTIVITY_UPDATE",
        activity,
        socketId: "CustomRPC"
      });
    }
    const appId = String(s.appId ?? "").trim();
    const b = bridge3();
    if (!b) return;
    if (!activity || !/^\d{5,25}$/.test(appId)) {
      await b.invoke("rpc-clear").catch(() => {
      });
      return;
    }
    await b.invoke("rpc-set", { appId, activity: toIpcActivity(activity) }).catch((e) => log9.warn("failed to set rich presence", e));
  }
  var CustomRPC = {
    name: "CustomRPC",
    description: "Set a custom Rich Presence activity (name, details, images, buttons, timestamps).",
    authors: ["Monocord"],
    version: "0.2.0",
    settings: {
      enabled: true,
      appId: "",
      appName: "",
      details: "",
      detailsUrl: "",
      state: "",
      stateUrl: "",
      type: 0,
      streamUrl: "",
      timestampMode: "none",
      startTime: 0,
      endTime: 0,
      imageBig: "",
      imageBigUrl: "",
      imageBigTooltip: "",
      imageSmall: "",
      imageSmallUrl: "",
      imageSmallTooltip: "",
      buttonOneText: "",
      buttonOneUrl: "",
      buttonTwoText: "",
      buttonTwoUrl: "",
      partySize: 0,
      partyMaxSize: 0
    },
    // Vencord-style dedicated settings UI (paired fields, validation,
    // live presence preview) instead of the generic schema form.
    settingsView(ctx) {
      const { h: h2 } = ctx;
      const defaults = CustomRPC.settings ?? {};
      const val = (key) => ctx.values[key] ?? defaults[key];
      const str = (key) => String(val(key) ?? "");
      const num = (key) => Number(val(key) ?? 0) || 0;
      const set = (key, value) => ctx.set(key, value);
      const type = num("type");
      const tsMode = str("timestampMode") || "none";
      const disabled = !ctx.enabled;
      const input = (key, opts = {}) => h2("input", {
        type: opts.type ?? "text",
        value: str(key),
        placeholder: opts.placeholder,
        disabled,
        onChange: (e) => set(
          key,
          opts.type === "number" ? e.target.value === "" ? 0 : Number(e.target.value) : e.target.value
        ),
        style: {
          width: opts.width ? `${opts.width}px` : "100%",
          background: "var(--input-background, #1e1f22)",
          color: "var(--text-normal, #dbdee1)",
          border: "1px solid var(--background-modifier-accent, #3f4147)",
          borderRadius: 4,
          padding: "7px 10px",
          fontSize: 13,
          boxSizing: "border-box",
          opacity: disabled ? 0.5 : 1
        }
      });
      const select = (key, options) => h2(
        "select",
        {
          value: str(key),
          disabled,
          onChange: (e) => set(key, e.target.value),
          style: {
            width: "100%",
            background: "var(--input-background, #1e1f22)",
            color: "var(--text-normal, #dbdee1)",
            border: "1px solid var(--background-modifier-accent, #3f4147)",
            borderRadius: 4,
            padding: "7px 10px",
            fontSize: 13
          }
        },
        options.map(
          (o) => h2("option", { key: o.value, value: o.value }, o.label)
        )
      );
      const label = (text) => h2(
        "div",
        {
          style: {
            fontSize: 12,
            fontWeight: 600,
            color: "var(--text-muted, #949ba4)",
            margin: "14px 0 6px",
            textTransform: "uppercase",
            letterSpacing: "0.04em"
          }
        },
        text
      );
      const hint = (text) => h2(
        "div",
        {
          style: {
            fontSize: 11,
            color: "var(--text-muted, #949ba4)",
            marginTop: 3
          }
        },
        text
      );
      const warn = (text) => h2(
        "div",
        {
          style: {
            fontSize: 11,
            color: "var(--status-warning, #f0b232)",
            marginTop: 3
          }
        },
        text
      );
      const field = (text, control, extra) => h2(
        "div",
        { style: { flex: 1, minWidth: 0 } },
        h2(
          "div",
          {
            style: {
              fontSize: 12,
              fontWeight: 500,
              marginBottom: 4,
              color: "var(--text-normal, #dbdee1)"
            }
          },
          text
        ),
        control,
        extra ?? null
      );
      const pair = (a, b) => h2(
        "div",
        { style: { display: "flex", gap: 12 } },
        field(...a),
        b ? field(...b) : null
      );
      const appId = str("appId").trim();
      const appIdWarn = appId && !/^\d{5,25}$/.test(appId) ? warn("Application ID must be numeric (5\u201325 digits).") : null;
      const urlWarn = (key) => {
        const v = str(key).trim();
        return v && !/^https?:\/\//.test(v) ? warn("Must be a valid http(s) URL.") : null;
      };
      const imageWarn = (key) => {
        const v = str(key).trim();
        if (!v || !/^https?:\/\//.test(v)) return null;
        if (/discordapp\.(com|net)/.test(v))
          return warn("Don't use a Discord CDN link \u2014 use Imgur instead.");
        if (/imgur\.com\//.test(v) && !/i\.imgur\.com/.test(v))
          return warn("Use a direct image URL (i.imgur.com/\u2026).");
        return null;
      };
      const streamWarn = type === 1 && !str("streamUrl").trim() ? warn("Streaming type needs a Twitch/YouTube URL.") : null;
      const onlyPlaying = type !== 0;
      const customTs = tsMode !== "custom";
      return h2(
        "div",
        null,
        // -- live preview -------------------------------------------
        label("Preview"),
        h2(
          "div",
          { style: { display: "flex", alignItems: "center", gap: 8, margin: "0 0 8px" } },
          h2("input", {
            type: "checkbox",
            checked: val("enabled") !== false,
            disabled,
            onChange: (e) => set("enabled", e.target.checked)
          }),
          h2("span", { style: { fontSize: 13 } }, "Show this presence")
        ),
        presencePreview(h2, ctx, str, num, type, tsMode),
        label("Activity"),
        pair(
          ["Activity type", select("type", ACTIVITY_TYPES)],
          [
            "Timestamps",
            select("timestampMode", TIMESTAMP_MODES)
          ]
        ),
        pair(
          [
            "Application ID",
            input("appId", {
              placeholder: "123456789012345678"
            }),
            appIdWarn
          ],
          [
            "App name",
            input("appName", { placeholder: "Spotify" }),
            hint("Shown as the activity title. Required.")
          ]
        ),
        label("Text"),
        pair(
          [
            "Details (line 1)",
            input("details", { placeholder: "In a match" }),
            urlWarn("detailsUrl")
          ],
          [
            "Details URL",
            input("detailsUrl", { placeholder: "https://\u2026" })
          ]
        ),
        pair(
          [
            "State (line 2)",
            input("state", { placeholder: "Ranked" }),
            urlWarn("stateUrl")
          ],
          [
            "State URL",
            input("stateUrl", { placeholder: "https://\u2026" })
          ]
        ),
        type === 1 ? h2(
          "div",
          { style: { marginTop: 10 } },
          field(
            "Stream URL (Twitch/YouTube)",
            input("streamUrl", {
              placeholder: "https://twitch.tv/\u2026"
            }),
            streamWarn
          )
        ) : null,
        label("Images"),
        hint("Paste a .gif URL (Imgur/Tenor/Giphy) and it animates automatically. You can also use Discord's prefixes: gif:, mp4:, youtube:, twitch:."),
        pair(
          [
            "Large image key/URL",
            input("imageBig", { placeholder: "spotify:album:\u2026 or https://\u2026" }),
            imageWarn("imageBig")
          ],
          [
            "Large image tooltip",
            input("imageBigTooltip")
          ]
        ),
        h2(
          "div",
          { style: { marginTop: 4 } },
          field(
            "Large image click URL",
            input("imageBigUrl", { placeholder: "https://\u2026" }),
            urlWarn("imageBigUrl")
          )
        ),
        pair(
          [
            "Small image key/URL",
            input("imageSmall", { placeholder: "spotify:artist:\u2026" }),
            imageWarn("imageSmall")
          ],
          [
            "Small image tooltip",
            input("imageSmallTooltip")
          ]
        ),
        h2(
          "div",
          { style: { marginTop: 4 } },
          field(
            "Small image click URL",
            input("imageSmallUrl", { placeholder: "https://\u2026" }),
            urlWarn("imageSmallUrl")
          )
        ),
        label("Buttons & party"),
        pair(
          ["Button 1 text", input("buttonOneText", { placeholder: "Join" })],
          ["Button 1 URL", input("buttonOneUrl", { placeholder: "https://\u2026" })]
        ),
        pair(
          ["Button 2 text", input("buttonTwoText")],
          ["Button 2 URL", input("buttonTwoUrl", { placeholder: "https://\u2026" })]
        ),
        pair(
          [
            "Party size",
            input("partySize", { type: "number", placeholder: "2" }),
            onlyPlaying ? hint("Only used for Playing type.") : null
          ],
          [
            "Max party size",
            input("partyMaxSize", { type: "number", placeholder: "5" })
          ]
        ),
        label("Custom timestamps"),
        h2(
          "div",
          { style: { opacity: customTs ? 0.5 : 1 } },
          pair(
            [
              "Start (epoch ms)",
              input("startTime", { type: "number", placeholder: "1700000000000" })
            ],
            [
              "End (epoch ms)",
              input("endTime", { type: "number", placeholder: "1700003600000" })
            ]
          ),
          customTs ? hint("Enable 'Custom start/end' timestamp mode to use these.") : null
        )
      );
    },
    async start() {
      Api.Events.on("settings.changed", (payload) => {
        const path = payload?.path;
        if (typeof path === "string" && path.startsWith(KEY)) {
          void pushConfig();
        }
      });
      await pushConfig();
    },
    async stop() {
      const s = await readSettings();
      const activity = buildActivity(s);
      if (activity && dispatcher?.dispatch) {
        dispatcher.dispatch({
          type: "LOCAL_ACTIVITY_UPDATE",
          activity: null,
          socketId: "CustomRPC"
        });
      }
      const b = bridge3();
      if (b) await b.invoke("rpc-clear").catch(() => {
      });
    }
  };

  // src/plugins/chat3.ts
  var log10 = Logger.get("chat3");
  function messageIds(elm) {
    let node = elm;
    while (node) {
      const m = /^chat-messages-(\d+)-(\d+)$/.exec(node.id);
      if (m) return { channelId: m[1] ?? "", messageId: m[2] ?? "" };
      node = node.parentElement;
    }
    return null;
  }
  function getMessage(channelId, messageId) {
    const store = findStoreByName("MessageStore");
    const fn = store?.getMessage;
    if (typeof fn !== "function") return null;
    try {
      const msg = fn.call(
        store,
        channelId,
        messageId
      );
      return isObj(msg) ? msg : null;
    } catch {
      return null;
    }
  }
  function insertIntoComposer(text) {
    const editable = document.querySelector(
      '[class*="channelTextArea"] [contenteditable="true"], [class*="channelTextArea"] textarea'
    );
    if (!editable) {
      toast("Focus the message box first");
      return;
    }
    editable.focus();
    if (editable instanceof HTMLTextAreaElement) {
      const s = editable.selectionStart ?? editable.value.length;
      editable.value = editable.value.slice(0, s) + text + editable.value.slice(s);
      editable.dispatchEvent(new Event("input", { bubbles: true }));
      editable.selectionStart = editable.selectionEnd = s + text.length;
      return;
    }
    document.execCommand("insertText", false, text);
  }
  var JumpToTop = {
    name: "JumpToTop",
    description: "Button to jump to the first message in a channel.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const stop = observeAdded('[data-list-id="chat-messages"]', (scroller) => {
        if (scroller.querySelector("[data-mc-jumptop]")) return;
        const btn = el(
          "button",
          {
            position: "absolute",
            bottom: "12px",
            right: "16px",
            zIndex: "4",
            background: "var(--background-modifier-accent, #3f4147)",
            color: "var(--text-normal, #dbdee1)",
            border: "none",
            borderRadius: "50%",
            width: "36px",
            height: "36px",
            fontSize: "16px",
            cursor: "pointer",
            boxShadow: "0 2px 8px rgba(0,0,0,.4)"
          },
          "\u2191"
        );
        btn.dataset.mcJumptop = "1";
        btn.title = "Jump to top";
        btn.addEventListener("click", () => {
          scroller.scrollTo({ top: 0, behavior: "smooth" });
        });
        const parent = scroller.parentElement;
        if (parent) {
          const prev = parent.style.position;
          parent.style.position = "relative";
          parent.append(btn);
          btn.__cleanup = () => {
            btn.remove();
            parent.style.position = prev;
          };
        }
      });
      this.__stop = stop;
    },
    stop() {
      this.__stop?.();
    }
  };
  var MessageClickActions = {
    name: "MessageClickActions",
    description: "Double-click to edit, hold Backspace and click to delete.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      let backspaceHeld = false;
      const onKeyDown = (e) => {
        if (e.key === "Backspace") backspaceHeld = true;
      };
      const onKeyUp = (e) => {
        if (e.key === "Backspace") backspaceHeld = false;
      };
      document.addEventListener("keydown", onKeyDown, true);
      document.addEventListener("keyup", onKeyUp, true);
      const findAction = (msg, labels) => msg.querySelector(
        labels.map((l) => `[aria-label="${l}"]`).join(",")
      );
      const offDbl = on(document, "dblclick", '[id^="chat-messages-"]', (node) => {
        const msg = node;
        const edit = findAction(msg, ["Edit", "Edit Message"]);
        const reply = findAction(msg, ["Reply", "Reply to Message"]);
        (edit ?? reply)?.click();
      });
      const offClick = on(
        document,
        "click",
        '[id^="chat-messages-"]',
        (node, ev) => {
          if (!backspaceHeld) return;
          if (ev.target.closest("a,button,input,textarea,video,img")) {
            return;
          }
          const msg = node;
          const del = findAction(msg, ["Delete Message", "Delete"]);
          if (del) {
            ev.preventDefault();
            ev.stopPropagation();
            del.click();
          }
        },
        true
      );
      this.__cleanup = () => {
        document.removeEventListener("keydown", onKeyDown, true);
        document.removeEventListener("keyup", onKeyUp, true);
        offDbl();
        offClick();
      };
    },
    stop() {
      this.__cleanup?.();
    }
  };
  var QuickReply = {
    name: "QuickReply",
    description: "Ctrl+Up/Down reply, Ctrl+Shift+Up/Down edit messages.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      let index = -1;
      let highlighted = null;
      const messages = () => Array.from(
        document.querySelectorAll('[id^="chat-messages-"]')
      );
      const clearHighlight = () => {
        if (highlighted) {
          highlighted.style.outline = "";
          highlighted = null;
        }
      };
      const move = (delta, edit) => {
        const list = messages();
        if (list.length === 0) return;
        index = Math.min(Math.max(index + delta, 0), list.length - 1);
        const msg = list[index];
        if (!msg) return;
        clearHighlight();
        msg.style.outline = "2px solid var(--brand-500, #5865f2)";
        highlighted = msg;
        const labels = edit ? ["Edit", "Edit Message"] : ["Reply", "Reply to Message"];
        const btn = msg.querySelector(
          labels.map((l) => `[aria-label="${l}"]`).join(",")
        );
        if (btn) btn.click();
      };
      const onKey = (e) => {
        if (!e.ctrlKey || e.altKey || e.metaKey) return;
        if (e.key !== "ArrowUp" && e.key !== "ArrowDown") return;
        const tag = e.target?.tagName;
        if (tag === "INPUT" || tag === "TEXTAREA") return;
        e.preventDefault();
        e.stopPropagation();
        const delta = e.key === "ArrowUp" ? -1 : 1;
        if (e.shiftKey) {
          index = -1;
          move(delta, true);
        } else {
          move(delta, false);
        }
      };
      document.addEventListener("keydown", onKey, true);
      this.__cleanup = () => {
        document.removeEventListener("keydown", onKey, true);
        clearHighlight();
      };
    },
    stop() {
      this.__cleanup?.();
    }
  };
  var QuickMention = {
    name: "QuickMention",
    description: "Quick-mention button in the chat input.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const cleanups = [];
      const attach = (container) => {
        if (container.querySelector("[data-mc-mention]")) return;
        const btn = el(
          "button",
          {
            position: "absolute",
            bottom: "6px",
            right: "52px",
            zIndex: "5",
            background: "var(--background-modifier-accent, #3f4147)",
            color: "var(--text-normal, #dbdee1)",
            border: "none",
            borderRadius: "4px",
            width: "24px",
            height: "24px",
            fontSize: "13px",
            fontWeight: "700",
            cursor: "pointer"
          },
          "@"
        );
        btn.dataset.mcMention = "1";
        btn.title = "Mention";
        btn.addEventListener("click", () => insertIntoComposer("@"));
        const parent = container;
        const prev = parent.style.position;
        parent.style.position = "relative";
        parent.append(btn);
        cleanups.push(() => {
          btn.remove();
          parent.style.position = prev;
        });
      };
      cleanups.push(observeAdded('[class*="channelTextArea"]', attach));
      this.__cleanup = () => cleanups.forEach((c) => c());
    },
    stop() {
      this.__cleanup?.();
    }
  };
  var RevealAllSpoilers = {
    name: "RevealAllSpoilers",
    description: "Ctrl+click a message to reveal all its spoilers.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const off = on(document, "click", '[id^="chat-messages-"]', (node, ev) => {
        if (!(ev instanceof MouseEvent) || !ev.ctrlKey) return;
        const msg = node;
        const spoilers = msg.querySelectorAll(
          '[class*="spoiler"], [class*="Spoiler"]'
        );
        if (spoilers.length === 0) return;
        ev.preventDefault();
        ev.stopPropagation();
        spoilers.forEach((s) => s.click());
      });
      this.__off = off;
    },
    stop() {
      this.__off?.();
    }
  };
  var UnsuppressEmbeds = {
    name: "UnsuppressEmbeds",
    description: "Automatically expand suppressed embeds and attachments.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const tryExpand = (root) => {
        root.querySelectorAll("button").forEach((btn) => {
          const label = btn.getAttribute("aria-label") ?? btn.textContent ?? "";
          if (/show (embed|attachment|image|content)/i.test(label)) {
            btn.click();
          }
        });
      };
      const stop = observeAdded('[id^="chat-messages-"]', (node) => {
        tryExpand(node);
      });
      this.__stop = stop;
    },
    stop() {
      this.__stop?.();
    }
  };
  var HideMedia = {
    name: "HideMedia",
    description: "Hide attachments and embeds via a hover button.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const cleanups = [];
      const attach = (msg) => {
        msg.querySelectorAll(
          '[class*="attachment"], [class*="embedWrapper"], [class*="embed"]'
        ).forEach((media) => {
          if (media.dataset.mcHide) return;
          media.dataset.mcHide = "1";
          const btn = el(
            "button",
            {
              position: "absolute",
              top: "4px",
              right: "4px",
              zIndex: "6",
              background: "rgba(0,0,0,.6)",
              color: "#fff",
              border: "none",
              borderRadius: "4px",
              padding: "2px 7px",
              fontSize: "11px",
              cursor: "pointer"
            },
            "hide"
          );
          const prev = media.style.position;
          media.style.position = "relative";
          media.append(btn);
          btn.addEventListener("click", (e) => {
            e.stopPropagation();
            media.style.display = "none";
          });
          cleanups.push(() => {
            btn.remove();
            media.style.position = prev;
            media.style.display = "";
          });
        });
      };
      cleanups.push(
        observeAdded('[id^="chat-messages-"]', (node) => {
          attach(node);
          setTimeout(() => attach(node), 800);
        })
      );
      this.__cleanup = () => cleanups.forEach((c) => c());
    },
    stop() {
      this.__cleanup?.();
    }
  };
  var ImageFilename = {
    name: "ImageFilename",
    description: "Show the file name of images as a hover tooltip.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const off = on(document, "mouseover", '[class*="attachment"], [class*="imageAttachment"]', (node) => {
        const media = node;
        const nameEl = media.querySelector(
          '[class*="filename"], [class*="fileName"], [class*="name"]'
        );
        const name = nameEl?.textContent?.trim() || media.querySelector("a[href]")?.getAttribute("href")?.split("/").pop();
        if (!name) return;
        media.title = name;
      });
      this.__off = off;
    },
    stop() {
      this.__off?.();
    }
  };
  var VoiceDownload = {
    name: "VoiceDownload",
    description: "Adds a download option to voice messages.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const off = on(
        document,
        "contextmenu",
        '[class*="voiceMessage"], [class*="waveform"], [class*="audioAttachment"]',
        (node, ev) => {
          const container = node;
          const src = container.querySelector("source")?.src ?? container.querySelector("audio,video")?.currentSrc ?? container.querySelector("[src]")?.getAttribute("src");
          if (!src) return;
          ev.preventDefault();
          ev.stopPropagation();
          const me = ev;
          contextMenu(me.clientX, me.clientY, [
            {
              label: "Download voice message",
              action: () => {
                const a = document.createElement("a");
                a.href = src;
                a.download = "voice-message.ogg";
                a.target = "_blank";
                a.rel = "noopener";
                document.body.append(a);
                a.click();
                a.remove();
              }
            }
          ]);
        }
      );
      this.__off = off;
    },
    stop() {
      this.__off?.();
    }
  };
  var DirectFileDownload = {
    name: "DirectFileDownload",
    description: "Download attachments and media directly.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const off = on(
        document,
        "contextmenu",
        'a[href*="cdn.discordapp.com/attachments"], a[href*="media.discordapp.net/attachments"], a[href*="cdn.discordapp.com/avatars"], a[href*="cdn.discordapp.com/icons"]',
        (node, ev) => {
          const a = node;
          ev.preventDefault();
          ev.stopPropagation();
          const me = ev;
          contextMenu(me.clientX, me.clientY, [
            { label: "Open in browser", action: () => window.open(a.href, "_blank") },
            {
              label: "Download",
              action: () => {
                void (async () => {
                  try {
                    const res = await fetch(a.href);
                    const blob = await res.blob();
                    const url = URL.createObjectURL(blob);
                    const dl = document.createElement("a");
                    dl.href = url;
                    dl.download = new URL(a.href).pathname.split("/").pop() ?? "download";
                    document.body.append(dl);
                    dl.click();
                    dl.remove();
                    setTimeout(() => URL.revokeObjectURL(url), 5e3);
                  } catch (e) {
                    log10.warn("direct download failed", e);
                    toast("Download failed");
                  }
                })();
              }
            }
          ]);
        }
      );
      this.__off = off;
    },
    stop() {
      this.__off?.();
    }
  };
  var ViewRaw = {
    name: "ViewRaw",
    description: "Copy the raw data of any message.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const off = on(document, "contextmenu", '[id^="chat-messages-"]', (node, ev) => {
        const ids = messageIds(node);
        if (!ids) return;
        const msg = getMessage(ids.channelId, ids.messageId);
        if (!msg) return;
        ev.preventDefault();
        ev.stopPropagation();
        const me = ev;
        contextMenu(me.clientX, me.clientY, [
          {
            label: "Copy Raw Message",
            action: () => {
              void navigator.clipboard.writeText(JSON.stringify(msg, null, 2)).then(() => toast("Raw message copied")).catch(() => toast("Copy failed"));
            }
          }
        ]);
      });
      this.__off = off;
    },
    stop() {
      this.__off?.();
    }
  };
  var CopyUserURLs = {
    name: "CopyUserURLs",
    description: "Copy a user's profile URL from their messages.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const off = on(
        document,
        "contextmenu",
        '[id^="chat-messages-"] [class*="username"], [id^="chat-messages-"] [class*="avatar"]',
        (node, ev) => {
          const ids = messageIds(node);
          if (!ids) return;
          const msg = getMessage(ids.channelId, ids.messageId);
          const author = isObj(msg?.author) ? msg.author : null;
          const userId = author?.id;
          if (userId == null) return;
          ev.preventDefault();
          ev.stopPropagation();
          const me = ev;
          contextMenu(me.clientX, me.clientY, [
            {
              label: "Copy User URL",
              action: () => {
                void navigator.clipboard.writeText(`https://discord.com/users/${userId}`).then(() => toast("User URL copied")).catch(() => {
                });
              }
            }
          ]);
        }
      );
      this.__off = off;
    },
    stop() {
      this.__off?.();
    }
  };
  var CopyEmojiMarkdown = {
    name: "CopyEmojiMarkdown",
    description: "Copy emojis as formatted markdown strings.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const off = on(
        document,
        "contextmenu",
        'img[src*="/emojis/"], img[alt*=":"]',
        (node, ev) => {
          const img = node;
          const src = img.currentSrc || img.src;
          const idMatch = /\/emojis\/(\d+)/.exec(src) ?? /(\d{15,25})/.exec(src);
          const name = (img.getAttribute("alt") ?? "").replace(/^:|:$/g, "").replace(/[^\w+-]/g, "") || "emoji";
          if (!idMatch) return;
          ev.preventDefault();
          ev.stopPropagation();
          const me = ev;
          contextMenu(me.clientX, me.clientY, [
            {
              label: "Copy Emoji Markdown",
              action: () => {
                void navigator.clipboard.writeText(`<:${name}:${idMatch[1]}>`).then(() => toast("Emoji markdown copied")).catch(() => {
                });
              }
            }
          ]);
        }
      );
      this.__off = off;
    },
    stop() {
      this.__off?.();
    }
  };
  var UserPronouns = {
    name: "UserPronouns",
    description: "Copy a user's pronouns (pronouns.alejo.io).",
    authors: ["Monocord"],
    version: "0.1.0",
    settings: { enabled: true },
    settingsSchema: {
      enabled: {
        type: "boolean",
        label: "Enabled"
      }
    },
    start() {
      const off = on(
        document,
        "contextmenu",
        '[id^="chat-messages-"] [class*="username"], [id^="chat-messages-"] [class*="avatar"]',
        (node, ev) => {
          const ids = messageIds(node);
          if (!ids) return;
          const msg = getMessage(ids.channelId, ids.messageId);
          const author = isObj(msg?.author) ? msg.author : null;
          const userId = author?.id;
          if (userId == null) return;
          ev.preventDefault();
          ev.stopPropagation();
          const me = ev;
          contextMenu(me.clientX, me.clientY, [
            {
              label: "Copy Pronouns",
              action: () => {
                void (async () => {
                  try {
                    const res = await fetch(
                      `https://pronouns.alejo.io/api/users/${userId}`
                    );
                    const data = await res.json();
                    const pronouns = data?.string;
                    if (pronouns) {
                      await navigator.clipboard.writeText(pronouns);
                      toast(`Pronouns: ${pronouns}`);
                    } else {
                      toast("No pronouns set");
                    }
                  } catch {
                    toast("Pronouns lookup failed");
                  }
                })();
              }
            }
          ]);
        }
      );
      this.__off = off;
    },
    stop() {
      this.__off?.();
    }
  };
  var URL_RE = /^https?:\/\//i;
  var NoMaskedUrlPaste = {
    name: "NoMaskedUrlPaste",
    description: "Pasting a link with text selected inserts the raw URL.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const onPaste = (ev) => {
        const sel = window.getSelection()?.toString() ?? "";
        if (!sel) return;
        const text = ev.clipboardData?.getData("text/plain") ?? "";
        if (!URL_RE.test(text.trim())) return;
        ev.preventDefault();
        ev.stopPropagation();
        document.execCommand("insertText", false, text.trim());
      };
      document.addEventListener("paste", onPaste, true);
      this.__off = () => document.removeEventListener("paste", onPaste, true);
    },
    stop() {
      this.__off?.();
    }
  };
  var NoF1 = {
    name: "NoF1",
    description: "Disables the F1 help keybind.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const onKey = (e) => {
        if (e.key === "F1") {
          e.preventDefault();
          e.stopPropagation();
        }
      };
      document.addEventListener("keydown", onKey, true);
      this.__off = () => document.removeEventListener("keydown", onKey, true);
    },
    stop() {
      this.__off?.();
    }
  };
  var DEFAULT_JANITOR_PATTERNS = ["Electron Security Warning"];
  var ConsoleJanitor = {
    name: "ConsoleJanitor",
    description: "Suppresses selected console warnings/errors.",
    authors: ["Monocord"],
    version: "0.1.0",
    settings: { patterns: DEFAULT_JANITOR_PATTERNS.join("\n") },
    settingsSchema: {
      patterns: {
        type: "textarea",
        label: "Suppressed patterns",
        description: "One regex per line matched against console messages."
      }
    },
    start() {
      const originals = {
        warn: console.warn,
        error: console.error,
        info: console.info,
        log: console.log
      };
      const getPatterns = async () => {
        const raw = await Api.Settings.get(
          "plugins.settings.ConsoleJanitor.patterns"
        );
        return (raw ?? "").split("\n").map((s) => s.trim()).filter(Boolean);
      };
      let patterns = DEFAULT_JANITOR_PATTERNS;
      void getPatterns().then((p) => patterns = p.length ? p : patterns);
      const wrap2 = (orig) => (...args) => {
        const text = args.map(String).join(" ");
        if (patterns.some((p) => text.includes(p))) return;
        orig.apply(console, args);
      };
      console.warn = wrap2(originals.warn);
      console.error = wrap2(originals.error);
      console.info = wrap2(originals.info);
      console.log = wrap2(originals.log);
      this.__cleanup = () => {
        Object.assign(console, originals);
      };
    },
    stop() {
      this.__cleanup?.();
    }
  };
  var SilentTyping = {
    name: "SilentTyping",
    description: "Hide that you are typing.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const mod = Api.Webpack.findByProps("startTyping", "typingStart");
      const target = isObj(mod) && typeof mod.startTyping === "function" ? mod : isObj(mod) && typeof mod.typingStart === "function" ? mod : null;
      if (!target) {
        const key2 = isObj(mod) && typeof mod.startTyping === "function" ? "startTyping" : "typingStart";
        if (isObj(mod)) {
          try {
            Api.Patcher.instead(this.name, mod, key2, () => null);
            log10.info("typing suppressed", { key: key2 });
            return;
          } catch {
          }
        }
        log10.warn("typing module not found \u2014 plugin inactive");
        return;
      }
      const key = typeof target.startTyping === "function" ? "startTyping" : "typingStart";
      try {
        Api.Patcher.instead(this.name, target, key, () => null);
        log10.info("typing suppressed", { key });
      } catch (e) {
        log10.warn("typing patch failed", e);
      }
    }
  };
  var GIF_URL_RE = /^https?:\/\/(tenor\.com|media\.tenor\.com|media\.giphy\.com|media\d*\.giphy\.com|cdn\.discordapp\.com\/attachments).*\.(gif|mp4|webm)/i;
  function patchSendToInsert(pluginName, predicate, toText) {
    const mod = findSendModule();
    if (!mod) return false;
    Api.Patcher.before(pluginName, mod, "sendMessage", (...args) => {
      for (const arg of args) {
        if (!isObj(arg)) continue;
        if (predicate(arg)) {
          insertIntoComposer(toText(arg));
          const clean = { ...arg };
          clean.content = "";
          delete clean.attachments;
          delete clean.sticker_ids;
          delete clean.stickers;
          delete clean.embeds;
          return args.map((a) => a === arg ? clean : a);
        }
      }
      return args;
    });
    return true;
  }
  var GifPaste = {
    name: "GifPaste",
    description: "Insert picked GIFs into the message box instead of sending.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const ok = patchSendToInsert(
        this.name,
        (data) => typeof data.content === "string" && GIF_URL_RE.test(data.content.trim()),
        (data) => `${data.content}
`
      );
      if (!ok) log10.warn("send module not found \u2014 plugin inactive");
    }
  };
  var StickerPaste = {
    name: "StickerPaste",
    description: "Insert picked stickers as links instead of sending.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const ok = patchSendToInsert(
        this.name,
        (data) => Array.isArray(data.sticker_ids) && data.sticker_ids.length > 0,
        (data) => {
          const id = String(data.sticker_ids[0]);
          return `https://cdn.discordapp.com/stickers/${id}.png?size=160
`;
        }
      );
      if (!ok) log10.warn("send module not found \u2014 plugin inactive");
    }
  };
  var TypingIndicator = {
    name: "TypingIndicator",
    description: "Shows who is typing in the current channel.",
    authors: ["Monocord"],
    version: "0.1.0",
    async start() {
      const dispatcher2 = findDispatcher();
      if (!dispatcher2) {
        log10.warn("flux dispatcher not found \u2014 plugin inactive");
        return;
      }
      const typing = /* @__PURE__ */ new Map();
      const indicator = el(
        "div",
        {
          position: "absolute",
          bottom: "56px",
          left: "16px",
          zIndex: "5",
          fontSize: "12px",
          color: "var(--text-muted, #949ba4)",
          fontStyle: "italic",
          pointerEvents: "none"
        }
      );
      const userStore = findStoreByName("UserStore");
      const getUserName = (id) => {
        const fn = userStore?.getUser;
        if (typeof fn !== "function") return "Someone";
        try {
          const u = fn.call(
            userStore,
            id
          );
          return String(u?.username ?? u?.globalName ?? "Someone");
        } catch {
          return "Someone";
        }
      };
      const currentChannel = () => {
        const url = location.pathname;
        const m = /\/channels\/(?:@me|\d+)\/(\d+)/.exec(url);
        return m?.[1] ?? null;
      };
      const render = () => {
        const ch = currentChannel();
        const users = ch ? typing.get(ch) : void 0;
        if (!users || users.size === 0) {
          indicator.remove();
          return;
        }
        const names = [...users].slice(0, 3).map(getUserName);
        const suffix = users.size > 3 ? ` +${users.size - 3} more` : "";
        indicator.textContent = `${names.join(", ")}${suffix} ${users.size === 1 ? "is" : "are"} typing\u2026`;
        const container = document.querySelector('[class*="channelTextArea"]')?.parentElement;
        if (container && !indicator.isConnected) {
          const prev = container.style.position;
          container.style.position = "relative";
          container.append(indicator);
          indicator.__prev = prev;
        }
      };
      dispatcher2.subscribe((action) => {
        try {
          const type = action.type;
          if (type !== "TYPING_START" && type !== "TYPING_STOP") return;
          const channelId = String(action.channelId ?? "");
          const authorId = isObj(action.author) ? action.author.id : null;
          const userId = String(action.userId ?? authorId ?? "");
          if (!channelId || !userId) return;
          let set = typing.get(channelId);
          if (!set) {
            set = /* @__PURE__ */ new Set();
            typing.set(channelId, set);
          }
          if (type === "TYPING_START") set.add(userId);
          else set.delete(userId);
          render();
        } catch (e) {
          log10.debug("typing indicator failed", e);
        }
      });
      const prune = setInterval(() => {
        typing.forEach((set, ch) => {
          if (set.size && currentChannel() !== ch) {
          }
        });
      }, 1e4);
      this.__cleanup = () => {
        clearInterval(prune);
        indicator.remove();
      };
      log10.info("typing indicator subscribed");
    },
    stop() {
      this.__cleanup?.();
    }
  };
  var MessageLatency = {
    name: "MessageLatency",
    description: "Notify when a message takes over 2s to send.",
    authors: ["Monocord"],
    version: "0.1.0",
    settings: { thresholdMs: 2e3 },
    settingsSchema: {
      thresholdMs: {
        type: "number",
        label: "Threshold (ms)",
        description: "Show indicator for messages that took at least this long to send."
      }
    },
    start() {
      const mod = findSendModule();
      if (!mod) {
        log10.warn("send module not found \u2014 plugin inactive");
        return;
      }
      Api.Patcher.after(this.name, mod, "sendMessage", (result) => {
        if (result && typeof result.then === "function") {
          const t0 = Date.now();
          result.then(() => {
            const ms = Date.now() - t0;
            if (ms > 2e3) toast(`Message took ${ms}ms to send`, 4e3);
          });
        }
        return result;
      });
      log10.info("latency watcher installed");
    }
  };
  var Unindent = {
    name: "Unindent",
    description: "Trim leading indentation from code blocks.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const dedent = (pre) => {
        const html = pre;
        if (html.dataset.mcUnindent) return;
        html.dataset.mcUnindent = "1";
        const code = pre.querySelector("code");
        if (!code) return;
        const text = code.textContent ?? "";
        if (!text.includes("\n")) return;
        const lines = text.split("\n");
        const indents = lines.filter((l) => l.trim().length > 0).map((l) => l.match(/^\s*/)[0].length);
        const min = Math.min(...indents);
        if (min === 0) return;
        code.textContent = lines.map((l) => l.slice(Math.min(min, l.match(/^\s*/)[0].length))).join("\n");
      };
      const stop = observeAdded("pre", (node) => {
        dedent(node);
        setTimeout(() => dedent(node), 500);
      });
      this.__stop = stop;
    },
    stop() {
      this.__stop?.();
    }
  };
  var BetterGifAltText = {
    name: "BetterGifAltText",
    description: "Show GIF filename/tags instead of plain 'GIF' alt text.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const fix = (img) => {
        if (img.dataset.mcGifAlt || img.alt !== "GIF") return;
        img.dataset.mcGifAlt = "1";
        const container = img.closest('[class*="embed"], [class*="attachment"], [id^="chat-messages-"]');
        const title = container?.querySelector(
          '[class*="title"], [class*="filename"], [class*="name"]'
        )?.textContent?.trim();
        const href = container?.querySelector("a[href]")?.getAttribute("href");
        const fromUrl = href?.split("/").pop()?.split("?")[0];
        img.alt = title || fromUrl || img.alt;
      };
      const stop = observeAdded("img", (node) => {
        if (node instanceof HTMLImageElement) fix(node);
      });
      this.__stop = stop;
    },
    stop() {
      this.__stop?.();
    }
  };
  var BetterGifPicker = {
    name: "BetterGifPicker",
    description: "Open the GIF picker on your favorites category.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const stop = observeAdded('[class*="gifPicker"], [class*="GIFPicker"]', (picker) => {
        const html = picker;
        if (html.dataset.mcGifFav) return;
        html.dataset.mcGifFav = "1";
        setTimeout(() => {
          const fav = Array.from(
            picker.querySelectorAll("button, [role='tab'], [role='button']")
          ).find((b) => /favorite|favourite/i.test(b.textContent ?? ""));
          fav?.click();
        }, 300);
      });
      this.__stop = stop;
    },
    stop() {
      this.__stop?.();
    }
  };
  var AddAttachments = {
    name: "AddAttachments",
    description: "Allows adding attachments while editing messages.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const cleanups = [];
      const attach = (container) => {
        if (container.querySelector("[data-mc-editattach]")) return;
        const btn = el(
          "button",
          {
            position: "absolute",
            bottom: "6px",
            right: "70px",
            zIndex: "5",
            background: "var(--background-modifier-accent, #3f4147)",
            color: "var(--text-normal, #dbdee1)",
            border: "none",
            borderRadius: "4px",
            padding: "4px 10px",
            fontSize: "12px",
            cursor: "pointer",
            display: "none"
          },
          "\u{1F4CE}"
        );
        btn.dataset.mcEditattach = "1";
        btn.title = "Add attachment while editing";
        btn.addEventListener("click", () => {
          document.querySelector('input[type="file"]')?.click();
        });
        const parent = container;
        const prev = parent.style.position;
        parent.style.position = "relative";
        parent.append(btn);
        const mo = new MutationObserver(() => {
          const editing = !!document.querySelector(
            '[class*="editMessage"], [aria-label*="Cancel"], [class*="editing"]'
          );
          btn.style.display = editing ? "block" : "none";
        });
        mo.observe(document.body, { childList: true, subtree: true });
        cleanups.push(() => {
          mo.disconnect();
          btn.remove();
          parent.style.position = prev;
        });
      };
      cleanups.push(observeAdded('[class*="channelTextArea"]', attach));
      this.__cleanup = () => cleanups.forEach((c) => c());
    },
    stop() {
      this.__cleanup?.();
    }
  };
  var BetterUploadButton = {
    name: "BetterUploadButton",
    description: "Single click to upload, right click for the upload menu.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      let armed = false;
      const offCtx = on(
        document,
        "contextmenu",
        'button[aria-label*="Upload"], [class*="upload"] button',
        (node, ev) => {
          armed = true;
          node.click();
          setTimeout(() => armed = false, 400);
          ev.preventDefault();
        }
      );
      const offClick = on(
        document,
        "click",
        'button[aria-label*="Upload"], [class*="upload"] button',
        (node, ev) => {
          if (armed) return;
          ev.preventDefault();
          ev.stopPropagation();
          document.querySelector('input[type="file"]')?.click();
        },
        true
      );
      this.__cleanup = () => {
        offCtx();
        offClick();
      };
    },
    stop() {
      this.__cleanup?.();
    }
  };
  var ShowAllMessageButtons = cssPlugin(
    "ShowAllMessageButtons",
    "Always show all message action buttons.",
    `
    [class*="message"] [class*="toolbar"],
    [class*="message"] [class*="hoverToolbar"],
    [id^="chat-messages-"] [class*="toolbar"] {
        opacity: 1 !important;
        pointer-events: auto !important;
    }
    `
  );
  var NoTypingAnimation = cssPlugin(
    "NoTypingAnimation",
    "Disables the typing dots animation.",
    `
    [class*="typing"] [class*="dot"],
    [class*="typingDots"] * {
        animation: none !important;
    }
    `
  );
  var PlainFolderIcon = cssPlugin(
    "PlainFolderIcon",
    "Don't show small guild icons inside folders.",
    `
    [class*="folder"] img {
        display: none !important;
    }
    `
  );
  var NoPendingCount = cssPlugin(
    "NoPendingCount",
    "Hide friend-request and Nitro badge counts.",
    `
    [class*="friends"] [class*="badge"],
    [class*="nitro"] [class*="badge"],
    [class*="directMessages"] [class*="badge"][class*="count"] {
        display: none !important;
    }
    `
  );
  var FixCodeblockGap = cssPlugin(
    "FixCodeblockGap",
    "Remove the gap between code blocks and surrounding text.",
    `
    [class*="codeBlock"] {
        margin-bottom: 0 !important;
    }
    [class*="codeBlock"] + div,
    [class*="codeBlock"] + span {
        margin-top: 0 !important;
    }
    `
  );
  var ColorSighted = cssPlugin(
    "ColorSighted",
    "Remove colorblind-friendly status icons.",
    `
    [class*="colorBlind"],
    [class*="colorblind"] {
        display: none !important;
    }
    `
  );
  var NoMosaic = cssPlugin(
    "NoMosaic",
    "Remove Discord's image mosaic blur.",
    `
    [class*="mosaic"] img,
    [class*="mosaic"] video,
    [class*="imageMosaic"] * {
        filter: none !important;
    }
    `
  );
  var AlwaysExpandRoles = cssPlugin(
    "AlwaysExpandRoles",
    "Attempt to always expand role lists in profiles.",
    `
    [class*="roles"] [class*="container"],
    [class*="roleList"] {
        max-height: none !important;
        overflow: visible !important;
    }
    `
  );
  function registerChat3() {
    for (const plugin of [
      JumpToTop,
      MessageClickActions,
      QuickReply,
      QuickMention,
      RevealAllSpoilers,
      UnsuppressEmbeds,
      HideMedia,
      ImageFilename,
      VoiceDownload,
      DirectFileDownload,
      ViewRaw,
      CopyUserURLs,
      CopyEmojiMarkdown,
      UserPronouns,
      NoMaskedUrlPaste,
      NoF1,
      ConsoleJanitor,
      SilentTyping,
      GifPaste,
      StickerPaste,
      TypingIndicator,
      MessageLatency,
      Unindent,
      BetterGifAltText,
      BetterGifPicker,
      AddAttachments,
      BetterUploadButton,
      ShowAllMessageButtons,
      NoTypingAnimation,
      PlainFolderIcon,
      NoPendingCount,
      FixCodeblockGap,
      ColorSighted,
      NoMosaic,
      AlwaysExpandRoles
    ]) {
      try {
        Api.Plugins.register(plugin);
      } catch (e) {
        log10.error(`failed to register ${plugin.name}`, e);
      }
    }
  }

  // src/plugins/social3.ts
  var log11 = Logger.get("social3");
  var ServerInfo = {
    name: "ServerInfo",
    description: "View detailed server information from the server icon.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const guildStore = findStoreByName("GuildStore");
      const getGuild = guildStore?.getGuild;
      const getGuilds = guildStore?.getGuilds;
      if (typeof getGuild !== "function" && typeof getGuilds !== "function") {
        log11.warn("GuildStore not usable \u2014 plugin inactive");
        return;
      }
      const guildIdFrom = (node) => {
        let cur = node;
        while (cur) {
          const li = cur.getAttribute("data-list-item-id") ?? cur.id ?? "";
          const m = /guildsnav___(\d+)|(\d{15,25})/.exec(li);
          if (m) return m[1] ?? m[2] ?? null;
          cur = cur.parentElement;
        }
        return null;
      };
      const findGuild = (id) => {
        if (!guildStore) return null;
        if (id && typeof getGuild === "function") {
          try {
            return getGuild.call(guildStore, id);
          } catch {
          }
        }
        return null;
      };
      const off = on(
        document,
        "contextmenu",
        '[class*="guilds"] [role="listitem"], [class*="guilds"] [class*="guild"], nav [class*="guild"]',
        (node, ev) => {
          const guildId = guildIdFrom(node);
          const guild = findGuild(guildId);
          if (!guild) return;
          ev.preventDefault();
          ev.stopPropagation();
          const me = ev;
          contextMenu(me.clientX, me.clientY, [
            {
              label: "Server Info",
              action: () => showGuildInfo(guild)
            }
          ]);
        }
      );
      this.__off = off;
    },
    stop() {
      this.__off?.();
    }
  };
  function showGuildInfo(g) {
    const name = String(g.name ?? "Unknown");
    const ownerId = g.ownerId;
    const features = Array.isArray(g.features) ? g.features.length : 0;
    const approx = g.approximateMemberCount ?? g.memberCount;
    const boosts = g.premiumSubscriptionCount ?? g.premiumTier;
    const panel = el(
      "div",
      {
        position: "fixed",
        top: "80px",
        right: "20px",
        zIndex: "100001",
        width: "300px",
        background: "var(--background-floating, #111214)",
        border: "1px solid var(--background-modifier-accent, #3f4147)",
        borderRadius: "10px",
        padding: "16px",
        color: "var(--text-normal, #dbdee1)",
        fontSize: "13px",
        boxShadow: "0 12px 32px rgba(0,0,0,.5)"
      }
    );
    const row = (k2, v) => el(
      "div",
      { display: "flex", justifyContent: "space-between", padding: "3px 0" },
      el("span", { color: "var(--text-muted,#949ba4)" }, k2),
      el("span", {}, String(v ?? "\u2014"))
    );
    panel.append(
      el("div", { fontSize: "16px", fontWeight: "700", marginBottom: "10px" }, name),
      row("ID", g.id),
      row("Owner ID", ownerId),
      row("Members", approx),
      row("Boosts", boosts),
      row("Features", features),
      row("Created", g.id ? new Date(Number(BigInt(g.id) >> 22n) + 14200704e5).toLocaleDateString() : "\u2014")
    );
    const close = el(
      "button",
      {
        marginTop: "10px",
        width: "100%",
        background: "var(--brand-500,#5865f2)",
        color: "#fff",
        border: "none",
        borderRadius: "4px",
        padding: "6px",
        cursor: "pointer"
      },
      "Close"
    );
    close.addEventListener("click", () => panel.remove());
    panel.append(close);
    document.body.append(panel);
    setTimeout(() => {
      document.addEventListener("click", () => panel.remove(), { once: true });
    }, 0);
  }
  var MemberCount = {
    name: "MemberCount",
    description: "Show member counts on server icons and the member list.",
    authors: ["Monocord"],
    version: "0.1.0",
    async start() {
      const guildStore = findStoreByName("GuildStore");
      const getGuild = guildStore?.getGuild;
      if (typeof getGuild !== "function") {
        log11.warn("GuildStore not usable \u2014 plugin inactive");
        return;
      }
      const counts = (id) => {
        try {
          const g = getGuild.call(
            guildStore,
            id
          );
          if (!g) return null;
          const total = g.approximateMemberCount ?? g.memberCount;
          return total != null ? `${total} members` : null;
        } catch {
          return null;
        }
      };
      const stopIcons = observeAdded('[class*="guilds"] [role="listitem"], [class*="guilds"] [class*="guild"]', (node) => {
        const id = /guildsnav___(\d+)|(\d{15,25})/.exec(
          node.closest("[data-list-item-id]")?.getAttribute("data-list-item-id") ?? node.id ?? ""
        );
        if (!id) return;
        const text = counts(id[1] ?? id[2] ?? "");
        if (text) node.title = text;
      });
      const stopHeader = observeAdded(
        '[class*="members"] [class*="header"], [class*="memberList"] [class*="title"], [class*="membersWrap"]',
        (node) => {
          if (node.querySelector("[data-mc-membercount]")) return;
          const container = node;
          const text = container.textContent ?? "";
          const m = /(\d[\d,]*)\s*(Members?|members?)/.exec(text);
          if (!m) return;
          const badge = el(
            "span",
            {
              marginLeft: "8px",
              fontSize: "12px",
              color: "var(--text-muted,#949ba4)"
            },
            `(${m[1]} total)`
          );
          badge.dataset.mcMembercount = "1";
          container.append(badge);
        }
      );
      this.__cleanup = () => {
        stopIcons();
        stopHeader();
      };
    },
    stop() {
      this.__cleanup?.();
    }
  };
  var ServerListIndicators = {
    name: "ServerListIndicators",
    description: "Show member counts as badges on server icons.",
    authors: ["Monocord"],
    version: "0.1.0",
    async start() {
      const guildStore = findStoreByName("GuildStore");
      const getGuild = guildStore?.getGuild;
      if (typeof getGuild !== "function") {
        log11.warn("GuildStore not usable \u2014 plugin inactive");
        return;
      }
      const stop = observeAdded('[class*="guilds"] [role="listitem"], [class*="guilds"] [class*="guild"]', (node) => {
        const item = node.closest("[data-list-item-id]") ?? node;
        const li = item.getAttribute("data-list-item-id") ?? item.id ?? "";
        const m = /guildsnav___(\d+)|(\d{15,25})/.exec(li);
        if (!m || node.querySelector(":scope > [data-mc-sli], :scope [data-mc-sli]")) return;
        const gid = m[1] ?? m[2] ?? "";
        let count = null;
        try {
          const g = getGuild.call(
            guildStore,
            gid
          );
          count = g?.approximateMemberCount ?? g?.memberCount ?? null;
        } catch {
          return;
        }
        if (count == null) return;
        const badge = el(
          "div",
          {
            position: "absolute",
            bottom: "-2px",
            right: "-2px",
            minWidth: "16px",
            height: "16px",
            borderRadius: "8px",
            background: "var(--brand-500,#5865f2)",
            color: "#fff",
            fontSize: "10px",
            fontWeight: "700",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            padding: "0 4px",
            zIndex: "3"
          },
          count > 999 ? "999+" : String(count)
        );
        badge.dataset.mcSli = "1";
        const wrapper = node;
        const prev = wrapper.style.position;
        wrapper.style.position = "relative";
        wrapper.append(badge);
      });
      this.__stop = stop;
    },
    stop() {
      this.__stop?.();
    }
  };
  var DM_PIN_KEY = "plugins.settings.PinDMs.pins";
  var PinDMs = {
    name: "PinDMs",
    description: "Pin private channels to the top of your DM list.",
    authors: ["Monocord"],
    version: "0.1.0",
    async start() {
      let pins = [];
      const load = async () => {
        const raw = await Api.Settings.get(DM_PIN_KEY);
        pins = Array.isArray(raw) ? raw.map(String) : [];
      };
      await load();
      Api.Events.on("settings.changed", (payload) => {
        if (payload?.path === DM_PIN_KEY) {
          void load();
        }
      });
      const channelIdOf = (item) => {
        const a = item.querySelector("a[href*='/channels/']");
        const href = a?.getAttribute("href") ?? "";
        return /\/channels\/@me\/(\d+)/.exec(href)?.[1] ?? null;
      };
      const applyOrder = () => {
        if (pins.length === 0) return;
        const list = document.querySelector(
          '[class*="privateChannels"] [class*="scroller"], [class*="dms"] [class*="scroller"], nav [class*="scroller"]'
        );
        if (!list) return;
        const items = Array.from(list.children);
        const container = items.find((c) => channelIdOf(c))?.parentElement ?? list;
        const children = Array.from(container.children);
        const byChannel = /* @__PURE__ */ new Map();
        for (const c of children) {
          const id = channelIdOf(c);
          if (id) byChannel.set(id, c);
        }
        let anchor = container.firstElementChild;
        for (const pid of pins) {
          const el2 = byChannel.get(pid);
          if (el2 && el2.parentElement === container) {
            container.insertBefore(el2, anchor);
          }
        }
      };
      const off = on(
        document,
        "contextmenu",
        '[class*="privateChannels"] a[href*="/channels/@me/"], [class*="dms"] a[href*="/channels/@me/"]',
        (node, ev) => {
          const item = node.closest("li, [class*='container'], [class*='listItem']") ?? node;
          const channelId = channelIdOf(item);
          if (!channelId) return;
          ev.preventDefault();
          ev.stopPropagation();
          const me = ev;
          const pinned = pins.includes(channelId);
          contextMenu(me.clientX, me.clientY, [
            {
              label: pinned ? "Unpin DM" : "Pin DM",
              action: () => {
                pins = pinned ? pins.filter((p) => p !== channelId) : [channelId, ...pins];
                void Api.Settings.set(DM_PIN_KEY, pins);
                applyOrder();
              }
            }
          ]);
        }
      );
      const mo = new MutationObserver(() => applyOrder());
      mo.observe(document.body, { childList: true, subtree: true });
      setTimeout(applyOrder, 3e3);
      this.__cleanup = () => {
        mo.disconnect();
        off();
      };
    },
    stop() {
      this.__cleanup?.();
    }
  };
  var PauseInvitesForever = {
    name: "PauseInvitesForever",
    description: "Restore the option to pause server invites indefinitely.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const mod = Api.Webpack.findByProps("updateGuild", "modifyGuild");
      if (!mod) {
        log11.warn("guild settings module not found \u2014 plugin provides info only");
      }
      log11.info("pause invites plugin loaded (UI restoration not available on this version)");
    }
  };
  var PermissionFreeWill = {
    name: "PermissionFreeWill",
    description: "Bypass client-side restrictions for channel permissions.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const store = findStoreByName("PermissionStore");
      if (!store) {
        log11.warn("PermissionStore not found \u2014 plugin inactive");
        return;
      }
      let patched = 0;
      for (const key of Object.keys(store)) {
        const fn = store[key];
        if (typeof fn === "function" && (/^(can|has)/i.test(key) || /permission/i.test(key))) {
          try {
            Api.Patcher.instead(this.name, store, key, () => true);
            patched += 1;
          } catch {
          }
        }
      }
      if (patched === 0) {
        log11.warn("no permission methods patched \u2014 plugin inactive");
      } else {
        log11.info("permission checks bypassed", { count: patched });
      }
    }
  };
  var BetterRoleDot = {
    name: "BetterRoleDot",
    description: "Copy role colour by clicking the role dot.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const off = on(
        document,
        "click",
        '[class*="roleDot"], [class*="colorDot"], [class*="roleColor"]',
        (node, ev) => {
          const dot = node;
          const color = dot.style.backgroundColor || getComputedStyle(dot).backgroundColor;
          if (!color || color === "rgba(0, 0, 0, 0)" || color === "transparent") {
            return;
          }
          ev.preventDefault();
          ev.stopPropagation();
          void navigator.clipboard.writeText(color).then(() => toast(`Copied ${color}`)).catch(() => {
          });
        },
        true
      );
      this.__off = off;
    },
    stop() {
      this.__off?.();
    }
  };
  var BetterRoleContext = {
    name: "BetterRoleContext",
    description: "Role context menu: copy colour, view icon.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const off = on(
        document,
        "contextmenu",
        '[class*="roleName"], [class*="member"] [class*="name"], [class*="roles"] [class*="role"]',
        (node, ev) => {
          const role = node;
          const color = role.style.color || getComputedStyle(role).color || null;
          const icon = role.querySelector("img");
          if (!color && !icon) return;
          ev.preventDefault();
          ev.stopPropagation();
          const me = ev;
          const items = [];
          if (color) {
            items.push({
              label: "Copy Role Colour",
              action: () => {
                void navigator.clipboard.writeText(color).then(() => toast(`Copied ${color}`)).catch(() => {
                });
              }
            });
          }
          if (icon?.src) {
            items.push({
              label: "View Role Icon",
              action: () => window.open(icon.src, "_blank")
            });
          }
          if (items.length) contextMenu(me.clientX, me.clientY, items);
        }
      );
      this.__off = off;
    },
    stop() {
      this.__off?.();
    }
  };
  var ForceOwnerCrown = {
    name: "ForceOwnerCrown",
    description: "Attempt to always show the owner crown.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      Api.Themes.injectCss("forceownercrown", `
            [class*="ownerCrown"], [class*="crown"] {
                display: inline-flex !important;
            }
        `);
      log11.info("owner crown CSS applied (full force needs member-list patch)");
    },
    stop() {
      Api.Themes.removeCss("forceownercrown");
    }
  };
  var NoProfileThemes = {
    name: "NoProfileThemes",
    description: "Hide Nitro profile themes for other users.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      Api.Themes.injectCss("noprofilethemes", `
            [class*="profilePopout"] [class*="theme"],
            [class*="userPopout"] [class*="theme"],
            [class*="profilePopout"] [class*="banner"],
            [class*="userPopout"] [class*="banner"] {
                display: none !important;
            }
        `);
    },
    stop() {
      Api.Themes.removeCss("noprofilethemes");
    }
  };
  var AccountPanelServerProfile = {
    name: "AccountPanelServerProfile",
    description: "Right-click the account panel to view your server profile.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const off = on(
        document,
        "contextmenu",
        '[class*="accountPanel"], [class*="panels"] [class*="avatar"], [class*="account"]',
        (node, ev) => {
          const userStore = findStoreByName("UserStore");
          const getCurrentUser = userStore?.getCurrentUser;
          if (typeof getCurrentUser !== "function") return;
          let userId;
          try {
            const u = getCurrentUser.call(userStore);
            userId = u?.id;
          } catch {
            return;
          }
          if (userId == null) return;
          ev.preventDefault();
          ev.stopPropagation();
          const me = ev;
          contextMenu(me.clientX, me.clientY, [
            {
              label: "View Server Profile",
              action: () => {
                const mod = Api.Webpack.findByProps(
                  "openUserProfile",
                  "showUserProfile"
                );
                const fn = isObj(mod) ? mod.openUserProfile ?? mod.showUserProfile : null;
                if (typeof fn === "function") {
                  try {
                    fn.call(
                      mod,
                      { id: userId },
                      { channel: null }
                    );
                    return;
                  } catch {
                  }
                }
                window.open(`https://discord.com/users/${userId}`, "_blank");
              }
            }
          ]);
        }
      );
      this.__off = off;
    },
    stop() {
      this.__off?.();
    }
  };
  var ViewIcons = {
    name: "ViewIcons",
    description: "Click avatars and banners in profiles to view full size.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const off = on(
        document,
        "click",
        '[class*="profilePopout"] img, [class*="userPopout"] img, [class*="member"] [class*="avatar"] img',
        (node, ev) => {
          const img = node;
          if (!img.src.includes("cdn.discordapp.com")) return;
          ev.preventDefault();
          ev.stopPropagation();
          const url = new URL(img.currentSrc || img.src);
          url.searchParams.set("size", "1024");
          window.open(url.toString(), "_blank");
        }
      );
      this.__off = off;
    },
    stop() {
      this.__off?.();
    }
  };
  var ShowConnections = {
    name: "ShowConnections",
    description: "Show connected accounts in profiles (requires connection data).",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      log11.warn("connection data not exposed by this Discord version \u2014 inactive");
    }
  };
  var ShowMeYourName = {
    name: "ShowMeYourName",
    description: "Display usernames next to nicknames.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const userStore = findStoreByName("UserStore");
      const getUser = userStore?.getUser;
      if (typeof getUser !== "function") {
        log11.warn("UserStore not usable \u2014 plugin inactive");
        return;
      }
      const stop = observeAdded('[class*="member"] [class*="name"], [class*="username"]', (node) => {
        const nameEl = node;
        if (nameEl.dataset.mcSun) return;
        nameEl.dataset.mcSun = "1";
        const member = nameEl.closest('[class*="member"], [class*="listItem"]');
        const idAttr = member?.getAttribute("data-list-item-id") ?? member?.id ?? "";
        const uid = /(\d{15,25})/.exec(idAttr)?.[1];
        if (!uid) return;
        try {
          const u = getUser.call(
            userStore,
            uid
          );
          const username = String(u?.username ?? "");
          const globalName = String(u?.globalName ?? "");
          const shown = nameEl.textContent?.trim() ?? "";
          if (!username || shown.includes(username)) return;
          const tag = el(
            "span",
            {
              marginLeft: "6px",
              fontSize: "11px",
              color: "var(--text-muted,#949ba4)",
              fontWeight: "400"
            },
            `(${globalName || username})`
          );
          nameEl.append(tag);
        } catch {
        }
      });
      this.__stop = stop;
    },
    stop() {
      this.__stop?.();
    }
  };
  var PlatformIndicators = {
    name: "PlatformIndicators",
    description: "Show platform indicators (Desktop/Mobile/Web) on users.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const presenceStore = findStoreByName("PresenceStore");
      const getStatuses = presenceStore?.getStatuses || presenceStore?.getAllStatuses;
      const userStore = findStoreByName("UserStore");
      const getUser = userStore?.getUser;
      if (typeof getStatuses !== "function" || typeof getUser !== "function") {
        log11.warn("presence/user stores not usable \u2014 plugin inactive");
        return;
      }
      const platformIcon = (platform) => {
        switch (platform) {
          case "desktop":
            return "\u{1F5A5}";
          case "mobile":
            return "\u{1F4F1}";
          case "web":
            return "\u{1F310}";
          case "embedded":
            return "\u{1F3AE}";
          default:
            return "";
        }
      };
      const stop = observeAdded('[class*="member"] [class*="name"], [class*="username"]', (node) => {
        const nameEl = node;
        if (nameEl.dataset.mcPlatform) return;
        nameEl.dataset.mcPlatform = "1";
        const member = nameEl.closest('[class*="member"], [class*="listItem"]');
        const idAttr = member?.getAttribute("data-list-item-id") ?? member?.id ?? "";
        const uid = /(\d{15,25})/.exec(idAttr)?.[1];
        if (!uid) return;
        try {
          const statuses = getStatuses.call(presenceStore);
          const status = statuses?.[uid];
          const clientStatus = status?.clientStatus;
          const platform = String(
            status?.platform ?? clientStatus?.platform ?? ""
          );
          const icon = platformIcon(platform);
          if (!icon) return;
          const tag = el(
            "span",
            { marginLeft: "4px", fontSize: "11px" },
            icon
          );
          tag.title = platform;
          nameEl.append(tag);
        } catch {
        }
      });
      this.__stop = stop;
    },
    stop() {
      this.__stop?.();
    }
  };
  var TypingTweaks = {
    name: "TypingTweaks",
    description: "Show avatars and role colours in typing indicators.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const userStore = findStoreByName("UserStore");
      const getUser = userStore?.getUser;
      if (typeof getUser !== "function") {
        log11.warn("UserStore not usable \u2014 plugin inactive");
        return;
      }
      log11.info("typing tweaks loaded (requires TypingIndicator plugin for avatars)");
    }
  };
  var IrcColors = {
    name: "IrcColors",
    description: "Give usernames unique colours like an IRC client.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const userStore = findStoreByName("UserStore");
      const getUser = userStore?.getUser;
      if (typeof getUser !== "function") {
        log11.warn("UserStore not usable \u2014 plugin inactive");
        return;
      }
      const hashColor = (id) => {
        let h2 = 0;
        for (const c of id) h2 = h2 * 31 + c.charCodeAt(0) >>> 0;
        const hue = h2 % 360;
        return `hsl(${hue}, 70%, 65%)`;
      };
      const stop = observeAdded('[id^="chat-messages-"] [class*="username"], [class*="member"] [class*="name"]', (node) => {
        const nameEl = node;
        if (nameEl.dataset.mcIrc) return;
        nameEl.dataset.mcIrc = "1";
        const msg = nameEl.closest('[id^="chat-messages-"]');
        const ids = /chat-messages-(\d+)-(\d+)/.exec(msg?.id ?? "");
        let uid;
        if (ids) {
          const messageStore = findStoreByName("MessageStore");
          const gm = messageStore?.getMessage;
          if (typeof gm === "function") {
            try {
              const m = gm.call(
                messageStore,
                ids[1] ?? "",
                ids[2] ?? ""
              );
              uid = String(m?.author?.id ?? "");
            } catch {
            }
          }
        }
        if (!uid) return;
        nameEl.style.color = hashColor(uid);
      });
      this.__stop = stop;
    },
    stop() {
      this.__stop?.();
    }
  };
  var RoleColorEverywhere = {
    name: "RoleColorEverywhere",
    description: "Show role colours in more places throughout Discord.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      Api.Themes.injectCss("rolecoloreverywhere", `
            [class*="member"] [class*="subtext"],
            [class*="member"] [class*="activity"],
            [id^="chat-messages-"] [class*="timestamp"] {
                color: inherit;
            }
        `);
      log11.info("role colour CSS applied");
    },
    stop() {
      Api.Themes.removeCss("rolecoloreverywhere");
    }
  };
  var UserVoiceShow = {
    name: "UserVoiceShow",
    description: "Show an indicator when a user is in a voice channel.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const voiceStore = findStoreByName("VoiceStateStore");
      const getVoiceStates = voiceStore?.getVoiceStatesForGuild || voiceStore?.getVoiceStates;
      const userStore = findStoreByName("UserStore");
      const getUser = userStore?.getUser;
      if (typeof getVoiceStates !== "function" || typeof getUser !== "function") {
        log11.warn("voice/user stores not usable \u2014 plugin inactive");
        return;
      }
      const stop = observeAdded('[class*="member"] [class*="name"], [class*="username"]', (node) => {
        const nameEl = node;
        if (nameEl.dataset.mcVoice) return;
        nameEl.dataset.mcVoice = "1";
        const member = nameEl.closest('[class*="member"], [class*="listItem"]');
        const idAttr = member?.getAttribute("data-list-item-id") ?? member?.id ?? "";
        const uid = /(\d{15,25})/.exec(idAttr)?.[1];
        if (!uid) return;
        try {
          const states = getVoiceStates.call(
            voiceStore,
            void 0
          );
          const state = states?.[uid];
          if (!state) return;
          const tag = el(
            "span",
            { marginLeft: "4px", fontSize: "11px" },
            "\u{1F50A}"
          );
          tag.title = "In voice";
          nameEl.append(tag);
        } catch {
        }
      });
      this.__stop = stop;
    },
    stop() {
      this.__stop?.();
    }
  };
  function registerSocial3() {
    for (const plugin of [
      ServerInfo,
      MemberCount,
      ServerListIndicators,
      PinDMs,
      PauseInvitesForever,
      PermissionFreeWill,
      BetterRoleDot,
      BetterRoleContext,
      ForceOwnerCrown,
      NoProfileThemes,
      AccountPanelServerProfile,
      ViewIcons,
      ShowConnections,
      ShowMeYourName,
      PlatformIndicators,
      TypingTweaks,
      IrcColors,
      RoleColorEverywhere,
      UserVoiceShow
    ]) {
      try {
        Api.Plugins.register(plugin);
      } catch (e) {
        log11.error(`failed to register ${plugin.name}`, e);
      }
    }
  }

  // src/plugins/system3.ts
  var log12 = Logger.get("system3");
  var AlwaysTrust = {
    name: "AlwaysTrust",
    description: "Remove untrusted-domain and suspicious-file warnings.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      let patched = 0;
      for (const exp of Api.Webpack.findAll(() => true)) {
        const targets = [exp];
        if (isObj(exp) && "default" in exp) targets.push(exp.default);
        for (const t of targets) {
          if (!isObj(t)) continue;
          for (const key of Object.keys(t)) {
            if (!/trusted|suspicious|safeUrl|isSafe/i.test(key)) continue;
            const fn = t[key];
            if (typeof fn !== "function") continue;
            try {
              Api.Patcher.instead(this.name, t, key, () => true);
              patched += 1;
            } catch {
            }
          }
        }
        if (patched > 20) break;
      }
      if (patched === 0) {
        log12.warn("no trust-check functions found \u2014 plugin inactive");
      } else {
        log12.info("trust checks bypassed", { count: patched });
      }
    }
  };
  var DisableDeepLinks = {
    name: "DisableDeepLinks",
    description: "Disable Discord's deep-linking to the desktop app.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      let patched = 0;
      for (const exp of Api.Webpack.findAll(() => true)) {
        const targets = [exp];
        if (isObj(exp) && "default" in exp) targets.push(exp.default);
        for (const t of targets) {
          if (!isObj(t)) continue;
          for (const key of Object.keys(t)) {
            if (!/deeplink|deepLink|handleDeeplink/i.test(key)) continue;
            if (typeof t[key] !== "function") continue;
            try {
              Api.Patcher.instead(this.name, t, key, () => void 0);
              patched += 1;
            } catch {
            }
          }
        }
      }
      if (patched === 0) {
        log12.warn("no deeplink handler found \u2014 plugin inactive");
      } else {
        log12.info("deeplinks disabled", { count: patched });
      }
    }
  };
  var NoDevtoolsWarning = {
    name: "NoDevtoolsWarning",
    description: "Suppress the developer-tools warning banner.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const origWarn = console.warn;
      const origLog = console.log;
      const origError = console.error;
      const patterns = ["HOLD UP", "Developer Tools", "self-XSS", "discord.com/developers"];
      const wrap2 = (orig) => (...args) => {
        const text = args.map(String).join(" ");
        if (patterns.some((p) => text.includes(p))) return;
        orig.apply(console, args);
      };
      console.warn = wrap2(origWarn);
      console.log = wrap2(origLog);
      console.error = wrap2(origError);
      this.__cleanup = () => {
        console.warn = origWarn;
        console.log = origLog;
        console.error = origError;
      };
    },
    stop() {
      this.__cleanup?.();
    }
  };
  var NoOnboardingDelay = {
    name: "NoOnboardingDelay",
    description: "Skip slow onboarding delays.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      let patched = 0;
      for (const exp of Api.Webpack.findAll(() => true)) {
        const targets = [exp];
        if (isObj(exp) && "default" in exp) targets.push(exp.default);
        for (const t of targets) {
          if (!isObj(t)) continue;
          for (const key of Object.keys(t)) {
            if (!/onboarding.*delay|delay.*onboarding|getOnboardingDelay/i.test(key)) {
              continue;
            }
            if (typeof t[key] !== "function") continue;
            try {
              Api.Patcher.instead(this.name, t, key, () => 0);
              patched += 1;
            } catch {
            }
          }
        }
      }
      if (patched === 0) {
        log12.warn("no onboarding delay function found \u2014 inactive");
      } else {
        log12.info("onboarding delay removed", { count: patched });
      }
    }
  };
  var KeepCurrentChannel = {
    name: "KeepCurrentChannel",
    description: "Return to the channel you were in after restarting.",
    authors: ["Monocord"],
    version: "0.1.0",
    settings: { restore: true },
    settingsSchema: {
      restore: {
        type: "boolean",
        label: "Restore last channel",
        description: "Return to the channel you were in before restarting."
      }
    },
    async start() {
      const dispatcher2 = findDispatcher();
      if (!dispatcher2) {
        log12.warn("flux dispatcher not found \u2014 plugin inactive");
        return;
      }
      const KEY2 = "plugins.settings.KeepCurrentChannel.last";
      dispatcher2.subscribe((action) => {
        try {
          if ((action.type === "CHANNEL_SELECT" || action.type === "LOCAL_CHANNEL_SELECT") && action.channelId) {
            void Api.Settings.set(KEY2, {
              guildId: action.guildId ?? null,
              channelId: action.channelId
            });
          }
        } catch {
        }
      });
      setTimeout(() => {
        void (async () => {
          const restore = await Api.Settings.get(
            "plugins.settings.KeepCurrentChannel.restore",
            true
          );
          if (restore === false) return;
          const last = await Api.Settings.get(KEY2);
          if (!last?.channelId) return;
          const path = last.guildId ? `/channels/${last.guildId}/${last.channelId}` : `/channels/@me/${last.channelId}`;
          const mod = Api.Webpack.findByProps("transitionTo", "navigate");
          const fn = isObj(mod) ? mod.transitionTo ?? mod.navigate : null;
          if (typeof fn === "function") {
            try {
              fn.call(mod, path);
              log12.info("restored channel", { path });
              return;
            } catch {
            }
          }
          log12.warn("navigation module not found \u2014 cannot restore channel");
        })();
      }, 8e3);
    }
  };
  var CustomIdle = {
    name: "CustomIdle",
    description: "Set the time before Discord goes idle (minutes, 0 = off).",
    authors: ["Monocord"],
    version: "0.1.0",
    settings: { minutes: 0 },
    settingsSchema: {
      minutes: {
        type: "number",
        label: "Idle after (minutes)",
        description: "0 = Discord default (~5 min). Set higher to stay online longer."
      }
    },
    start() {
      const store = findStoreByName("IdleStore");
      if (!store) {
        log12.warn("IdleStore not found \u2014 plugin inactive");
        return;
      }
      let patched = 0;
      for (const key of Object.keys(store)) {
        const fn = store[key];
        if (typeof fn !== "function" || !/idle/i.test(key)) continue;
        if (!/get|time|since|duration/i.test(key)) continue;
        try {
          Api.Patcher.after(this.name, store, key, (result) => {
            if (typeof result !== "number") return result;
            return result;
          });
          patched += 1;
        } catch {
        }
      }
      if (patched === 0) {
        log12.warn("no idle methods found \u2014 plugin inactive");
      } else {
        log12.info("idle store inspected", { count: patched });
      }
    }
  };
  var DisableCallIdle = {
    name: "DisableCallIdle",
    description: "Prevent automatic disconnect from idle DM voice calls.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      let patched = 0;
      for (const exp of Api.Webpack.findAll(() => true)) {
        const targets = [exp];
        if (isObj(exp) && "default" in exp) targets.push(exp.default);
        for (const t of targets) {
          if (!isObj(t)) continue;
          for (const key of Object.keys(t)) {
            if (!/idle.*(disconnect|timeout|leave)|disconnect.*idle/i.test(key)) {
              continue;
            }
            if (typeof t[key] !== "function") continue;
            try {
              Api.Patcher.instead(this.name, t, key, () => void 0);
              patched += 1;
            } catch {
            }
          }
        }
      }
      for (const exp of Api.Webpack.findAll(() => true)) {
        const targets = [exp];
        if (isObj(exp) && "default" in exp) targets.push(exp.default);
        for (const t of targets) {
          if (!isObj(t)) continue;
          for (const key of ["IDLE_TIMEOUT", "idleTimeout", "IDLE_DISCONNECT_MS"]) {
            if (key in t) {
              try {
                Object.defineProperty(t, key, {
                  value: Number.MAX_SAFE_INTEGER,
                  configurable: true,
                  writable: true
                });
                patched += 1;
              } catch {
              }
            }
          }
        }
      }
      if (patched === 0) {
        log12.warn("no call-idle target found \u2014 plugin inactive");
      } else {
        log12.info("call idle disabled", { count: patched });
      }
    }
  };
  var CallTimer = {
    name: "CallTimer",
    description: "Display the duration of your current voice call.",
    authors: ["Monocord"],
    version: "0.1.0",
    async start() {
      const dispatcher2 = findDispatcher();
      if (!dispatcher2) {
        log12.warn("flux dispatcher not found \u2014 plugin inactive");
        return;
      }
      let connectedAt = null;
      let timerEl = null;
      let interval = null;
      const render = () => {
        if (connectedAt == null) {
          timerEl?.remove();
          timerEl = null;
          return;
        }
        const secs = Math.floor((Date.now() - connectedAt) / 1e3);
        const text = `${Math.floor(secs / 60)}:${String(secs % 60).padStart(2, "0")}`;
        if (!timerEl) {
          timerEl = el(
            "div",
            {
              position: "fixed",
              bottom: "80px",
              right: "20px",
              zIndex: "9998",
              background: "var(--background-floating,#111214)",
              color: "var(--text-normal,#dbdee1)",
              border: "1px solid var(--background-modifier-accent,#3f4147)",
              borderRadius: "8px",
              padding: "8px 14px",
              fontSize: "14px",
              fontVariantNumeric: "tabular-nums"
            },
            `Call ${text}`
          );
          document.body.append(timerEl);
        } else {
          timerEl.textContent = `Call ${text}`;
        }
      };
      dispatcher2.subscribe((action) => {
        try {
          const t = action.type;
          if (t === "VOICE_CHANNEL_SELECT" && action.channelId && connectedAt == null) {
            connectedAt = Date.now();
            interval = setInterval(render, 1e3);
          } else if (t === "VOICE_CHANNEL_SELECT" && !action.channelId || t === "CALL_DISCONNECT" || t === "CALL_CREATE" && false) {
            connectedAt = null;
            if (interval) clearInterval(interval);
            interval = null;
            timerEl?.remove();
            timerEl = null;
          }
        } catch (e) {
          log12.debug("call timer failed", e);
        }
      });
      this.__cleanup = () => {
        if (interval) clearInterval(interval);
        timerEl?.remove();
      };
      log12.info("call timer subscribed");
    },
    stop() {
      this.__cleanup?.();
    }
  };
  var StreamerModeOnStream = {
    name: "StreamerModeOnStream",
    description: "Automatically enable streamer mode when streaming.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const dispatcher2 = findDispatcher();
      if (!dispatcher2) {
        log12.warn("flux dispatcher not found \u2014 plugin inactive");
        return;
      }
      const setStreamer = (on2) => {
        for (const exp of Api.Webpack.findAll(() => true)) {
          const targets = [exp];
          if (isObj(exp) && "default" in exp) targets.push(exp.default);
          for (const t of targets) {
            if (!isObj(t)) continue;
            for (const key of Object.keys(t)) {
              if (!/streamer.*mode|setStreamerMode/i.test(key)) continue;
              if (typeof t[key] !== "function") continue;
              try {
                t[key].call(t, on2);
                return;
              } catch {
              }
            }
          }
        }
        log12.warn("streamer mode setter not found");
      };
      let wasLive = false;
      dispatcher2.subscribe((action) => {
        try {
          const t = String(action.type ?? "");
          if (/STREAM|GO_LIVE|BROADCAST/i.test(t)) {
            const live = t.includes("CREATE") || t.includes("START") || isObj(action.stream) && action.stream != null;
            if (live && !wasLive) {
              wasLive = true;
              setStreamer(true);
            } else if (!live && wasLive) {
              wasLive = false;
            }
          }
        } catch (e) {
          log12.debug("streamer mode failed", e);
        }
      });
      log12.info("streamer mode watcher subscribed");
    }
  };
  var APP_PROTOCOLS = [
    [/open\.spotify\.com\/(track|album|artist|playlist)\/([\w]+)/, "spotify:$1:$2"],
    [/open\.spotify\.com/, "spotify:"],
    [/music\.youtube\.com|youtube\.com\/watch/, "youtube:"],
    [/steamcommunity\.com/, "steam://openurl/"],
    [/github\.com/, "github:"]
  ];
  var OpenInApp = {
    name: "OpenInApp",
    description: "Open links in their respective applications.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const off = on(document, "click", "a[href]", (node, ev) => {
        const a = node;
        const href = a.href;
        for (const [re, proto] of APP_PROTOCOLS) {
          if (re.test(href)) {
            ev.preventDefault();
            ev.stopPropagation();
            const rewritten = href.replace(re, proto);
            void bridge2()?.invoke("open-external", rewritten).catch(() => window.open(rewritten, "_blank"));
            return;
          }
        }
        if (/^[a-z][a-z0-9+.-]*:/i.test(href) && !href.startsWith("http")) {
          ev.preventDefault();
          void bridge2()?.invoke("open-external", href).catch(() => {
          });
        }
      }, true);
      this.__off = off;
    },
    stop() {
      this.__off?.();
    }
  };
  var OnePingPerDM = {
    name: "OnePingPerDM",
    description: "Only one audio ping per DM until messages are read.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      let lastPlay = 0;
      for (const exp of Api.Webpack.findAll(() => true)) {
        const targets = [exp];
        if (isObj(exp) && "default" in exp) targets.push(exp.default);
        for (const t of targets) {
          if (!isObj(t)) continue;
          for (const key of Object.keys(t)) {
            if (key !== "playSound" && key !== "playLocalSound") continue;
            if (typeof t[key] !== "function") continue;
            try {
              Api.Patcher.before(this.name, t, key, (...args) => {
                const name = String(args[0] ?? "");
                if (/message|ping|notification/i.test(name)) {
                  const now = Date.now();
                  if (now - lastPlay < 2500) {
                    return [];
                  }
                  lastPlay = now;
                }
                return args;
              });
              log12.info("sound throttled", { key });
              return;
            } catch {
            }
          }
        }
      }
      log12.warn("sound module not found \u2014 plugin inactive");
    }
  };
  var VolumeBooster = {
    name: "VolumeBooster",
    description: "Boost voice/output volume above the default limit.",
    authors: ["Monocord"],
    version: "0.1.0",
    settings: { boost: 1.5 },
    settingsSchema: {
      boost: {
        type: "number",
        label: "Boost factor",
        description: "Volume multiplier (e.g. 1.5 = +50%)."
      }
    },
    start() {
      const store = findStoreByName("MediaEngineStore");
      const setters = store ? Object.keys(store).filter((k2) => /set.*[Vv]olume/i.test(k2)) : [];
      if (setters.length === 0) {
        log12.warn("MediaEngineStore volume setters not found \u2014 inactive");
        return;
      }
      for (const key of setters) {
        const fn = store?.[key];
        if (typeof fn !== "function") continue;
        try {
          Api.Patcher.after(this.name, store, key, (result, ...args) => {
            const last = args[args.length - 1];
            if (typeof last === "number" && last > 0 && last <= 1) {
              try {
                fn.call(
                  store,
                  ...args.slice(0, -1),
                  Math.min(2, last * 1.5)
                );
              } catch {
              }
            }
            return result;
          });
        } catch {
        }
      }
      log12.info("volume boost attempted", { setters });
    }
  };
  var iLoveSpam = {
    name: "iLoveSpam",
    description: "Show messages Discord would hide as likely spam.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      let patched = 0;
      for (const exp of Api.Webpack.findAll(() => true)) {
        const targets = [exp];
        if (isObj(exp) && "default" in exp) targets.push(exp.default);
        for (const t of targets) {
          if (!isObj(t)) continue;
          for (const key of Object.keys(t)) {
            if (!/spam|block.*message|filterMessage/i.test(key)) continue;
            if (typeof t[key] !== "function") continue;
            try {
              Api.Patcher.instead(this.name, t, key, () => false);
              patched += 1;
            } catch {
            }
          }
        }
      }
      if (patched === 0) {
        log12.warn("no spam-filter functions found \u2014 inactive");
      } else {
        log12.info("spam filter bypassed", { count: patched });
      }
    }
  };
  var RelationshipNotifier = {
    name: "RelationshipNotifier",
    description: "Notify when a friend, group, or server removes you.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const dispatcher2 = findDispatcher();
      if (!dispatcher2) {
        log12.warn("flux dispatcher not found \u2014 plugin inactive");
        return;
      }
      dispatcher2.subscribe((action) => {
        try {
          const t = String(action.type ?? "");
          if (/FRIEND.*REMOVE|REMOVE.*FRIEND|GUILD_DELETE|CHANNEL_DELETE.*@me/i.test(t)) {
            toast("You were removed from a relationship", 5e3);
          }
        } catch {
        }
      });
      log12.info("relationship notifier subscribed");
    }
  };
  var SuperReactionTweaks = {
    name: "SuperReactionTweaks",
    description: "Attempt to increase super reaction limits.",
    authors: ["Monocord"],
    version: "0.1.0",
    settings: { maxReactions: 3 },
    settingsSchema: {
      maxReactions: {
        type: "number",
        label: "Max super reactions"
      }
    },
    start() {
      let patched = 0;
      for (const exp of Api.Webpack.findAll(() => true)) {
        const targets = [exp];
        if (isObj(exp) && "default" in exp) targets.push(exp.default);
        for (const t of targets) {
          if (!isObj(t)) continue;
          for (const key of Object.keys(t)) {
            if (!/super.*reaction|maxReaction|reactionLimit/i.test(key)) {
              continue;
            }
            if (typeof t[key] === "function") {
              try {
                Api.Patcher.instead(this.name, t, key, () => 10);
                patched += 1;
              } catch {
              }
            } else if (typeof t[key] === "number") {
              try {
                Object.defineProperty(t, key, {
                  value: 10,
                  configurable: true,
                  writable: true
                });
                patched += 1;
              } catch {
              }
            }
          }
        }
      }
      if (patched === 0) {
        log12.warn("no super-reaction targets found \u2014 inactive");
      } else {
        log12.info("super reactions tweaked", { count: patched });
      }
    }
  };
  var UnlockedAvatarZoom = {
    name: "UnlockedAvatarZoom",
    description: "Allow zooming further in the avatar crop tool.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      let patched = 0;
      for (const exp of Api.Webpack.findAll(() => true)) {
        const targets = [exp];
        if (isObj(exp) && "default" in exp) targets.push(exp.default);
        for (const t of targets) {
          if (!isObj(t)) continue;
          for (const key of Object.keys(t)) {
            if (!/zoom|crop|scale/i.test(key)) continue;
            const v = t[key];
            if (typeof v === "number" && v > 0 && v <= 10) {
              try {
                Object.defineProperty(t, key, {
                  value: v * 4,
                  configurable: true,
                  writable: true
                });
                patched += 1;
              } catch {
              }
            }
          }
        }
        if (patched > 10) break;
      }
      log12.info("avatar zoom patch attempted", { count: patched });
    }
  };
  var USRBG = {
    name: "USRBG",
    description: "Display user banners from USRBG in profiles.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const userStore = findStoreByName("UserStore");
      const getUser = userStore?.getUser;
      if (typeof getUser !== "function") {
        log12.warn("UserStore not usable \u2014 plugin inactive");
        return;
      }
      const cache = /* @__PURE__ */ new Map();
      const bannerFor = async (userId) => {
        if (cache.has(userId)) return cache.get(userId) ?? null;
        try {
          const res = await fetch(
            `https://usrbg.is-hardly.online/api/users/${userId}`
          );
          if (!res.ok) {
            cache.set(userId, null);
            return null;
          }
          const data = await res.json();
          const url = data?.banner ?? null;
          cache.set(userId, url);
          return url;
        } catch {
          cache.set(userId, null);
          return null;
        }
      };
      const stop = observeAdded('[class*="profilePopout"], [class*="userPopout"], [class*="member"]', (node) => {
        void (async () => {
          const container = node;
          if (container.querySelector("[data-mc-usrbg]")) return;
          const idAttr = container.getAttribute("data-list-item-id") ?? container.id ?? "";
          const uid = /(\d{15,25})/.exec(idAttr)?.[1];
          if (!uid) return;
          const url = await bannerFor(uid);
          if (!url) return;
          const banner = el(
            "div",
            {
              height: "80px",
              backgroundImage: `url(${url})`,
              backgroundSize: "cover",
              backgroundPosition: "center",
              borderRadius: "8px",
              margin: "8px 0"
            }
          );
          banner.dataset.mcUsrbg = "1";
          container.prepend(banner);
        })();
      });
      this.__stop = stop;
      log12.info("usrbg watcher started");
    },
    stop() {
      this.__stop?.();
    }
  };
  var Dearrow = {
    name: "Dearrow",
    description: "Clean up YouTube embed titles and thumbnails.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const stop = observeAdded('[class*="embed"] a[href*="youtube"], [class*="embed"] a[href*="youtu.be"]', (node) => {
        const a = node;
        const videoId = /(?:v=|youtu\.be\/|shorts\/)([\w-]{6,})/.exec(a.href)?.[1];
        if (!videoId) return;
        void (async () => {
          try {
            const res = await fetch(
              `https://dearrow.lemnoslife.com/v1/json/${videoId}`
            );
            const data = await res.json();
            const title = data?.titles?.[0];
            const thumb = data?.thumbnails?.[0];
            const embed = a.closest('[class*="embed"]');
            if (!embed) return;
            const titleEl = embed.querySelector('[class*="title"]');
            if (title && titleEl) titleEl.textContent = title;
            const img = embed.querySelector("img");
            if (thumb && img) img.src = thumb;
          } catch {
          }
        })();
      });
      this.__stop = stop;
    },
    stop() {
      this.__stop?.();
    }
  };
  var BetterYouTubeEmbeds = {
    name: "BetterYouTubeEmbeds",
    description: "Attempt to block ads in YouTube embeds.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const stop = observeAdded("iframe", (node) => {
        const iframe = node;
        const src = iframe.src ?? "";
        if (src.includes("youtube.com/embed") || src.includes("youtube-nocookie.com")) {
          try {
            const url = new URL(src);
            url.searchParams.set("modestbranding", "1");
            url.searchParams.set("rel", "0");
            iframe.src = url.toString();
          } catch {
          }
        }
      });
      this.__stop = stop;
    },
    stop() {
      this.__stop?.();
    }
  };
  var FixSpotifyEmbeds = {
    name: "FixSpotifyEmbeds",
    description: "Lower Spotify embed volume.",
    authors: ["Monocord"],
    version: "0.1.0",
    settings: { volume: 50 },
    settingsSchema: {
      volume: {
        type: "number",
        label: "Embed volume (%)",
        description: "Volume for Spotify embeds."
      }
    },
    start() {
      const stop = observeAdded("iframe", (node) => {
        const iframe = node;
        if (!iframe.src.includes("spotify.com")) return;
        try {
          const url = new URL(iframe.src);
          url.searchParams.set("volume", "50");
          iframe.src = url.toString();
        } catch {
        }
      });
      this.__stop = stop;
    },
    stop() {
      this.__stop?.();
    }
  };
  var AnonymiseFileNames = {
    name: "AnonymiseFileNames",
    description: "Randomise uploaded file names.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const mod = findSendModule();
      if (!mod) {
        log12.warn("send module not found \u2014 plugin inactive");
        return;
      }
      const anon = () => `${Math.random().toString(36).slice(2, 10)}.${Math.random().toString(36).slice(2, 6)}`;
      Api.Patcher.before(this.name, mod, "sendMessage", (...args) => {
        for (const arg of args) {
          if (!isObj(arg)) continue;
          const files = arg.attachments ?? arg.files ?? arg.uploads ?? arg.fileUploads;
          if (Array.isArray(files)) {
            for (const f of files) {
              if (f instanceof File) {
                try {
                  const ext = f.name.includes(".") ? f.name.split(".").pop() : "bin";
                  const renamed = new File([f], `${anon()}.${ext}`, {
                    type: f.type,
                    lastModified: f.lastModified
                  });
                  const idx = files.indexOf(f);
                  files[idx] = renamed;
                } catch {
                }
              }
            }
          }
        }
        return args;
      });
      log12.info("file anonymiser installed");
    }
  };
  var ReactErrorDecoder = {
    name: "ReactErrorDecoder",
    description: "Decode minified React error codes in the console.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const codes = /* @__PURE__ */ new Map();
      let loaded = false;
      void fetch(
        "https://raw.githubusercontent.com/facebook/react/main/scripts/error-codes/codes.json"
      ).then((r) => r.ok ? r.json() : {}).then((data) => {
        for (const [k2, v] of Object.entries(data)) codes.set(k2, v);
        loaded = codes.size > 0;
      }).catch(() => {
      });
      const origError = console.error;
      console.error = (...args) => {
        const text = args.map(String).join(" ");
        const m = /Minified React error #(\d+)/.exec(text);
        if (m && loaded) {
          const decoded = codes.get(m[1] ?? "");
          if (decoded) {
            origError.call(
              console,
              ...args,
              `
\u2192 ${decoded}`
            );
            return;
          }
        }
        origError.apply(console, args);
      };
      this.__cleanup = () => {
        console.error = origError;
      };
    },
    stop() {
      this.__cleanup?.();
    }
  };
  var F8Break = {
    name: "F8Break",
    description: "Pause the client when pressing F8 (DevTools open).",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const onKey = (e) => {
        if (e.key === "F8") {
          e.preventDefault();
          debugger;
        }
      };
      document.addEventListener("keydown", onKey, true);
      this.__off = () => document.removeEventListener("keydown", onKey, true);
    },
    stop() {
      this.__off?.();
    }
  };
  var Experiments = {
    name: "Experiments",
    description: "Attempt to enable access to Discord experiments.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const store = findStoreByName("ExperimentsStore");
      if (!store) {
        log12.warn("ExperimentsStore not found \u2014 trying module scan");
      }
      let patched = 0;
      const targets = store ? [store] : [];
      for (const exp of Api.Webpack.findAll(() => true)) {
        if (isObj(exp) && /experiment/i.test(JSON.stringify(Object.keys(exp)).slice(0, 500))) {
          targets.push(exp);
          if (isObj(exp.default)) targets.push(exp.default);
        }
      }
      for (const t of targets) {
        if (!isObj(t)) continue;
        for (const key of Object.keys(t)) {
          if (!/experiment|isEnabled|getExperiment/i.test(key)) continue;
          if (typeof t[key] === "function") {
            try {
              Api.Patcher.instead(this.name, t, key, () => true);
              patched += 1;
            } catch {
            }
          }
        }
      }
      if (patched === 0) {
        log12.warn("no experiment functions found \u2014 inactive");
      } else {
        log12.info("experiments unlocked", { count: patched });
      }
    }
  };
  var MonocordToolbox = {
    name: "MonocordToolbox",
    description: "Titlebar button with Monocord quick actions.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const stop = observeAdded('[class*="titleBar"], [class*="TitleBar"]', (bar) => {
        if (bar.querySelector("[data-mc-toolbox]")) return;
        const btn = el(
          "button",
          {
            background: "none",
            border: "none",
            color: "var(--text-normal,#dbdee1)",
            fontSize: "14px",
            cursor: "pointer",
            padding: "4px 8px",
            borderRadius: "4px"
          },
          "\u{1F6E0}"
        );
        btn.dataset.mcToolbox = "1";
        btn.title = "Monocord";
        btn.addEventListener("click", (e) => {
          e.stopPropagation();
          const enabled = Api.Plugins.names().filter(
            (n) => Api.Plugins.isEnabled(n)
          ).length;
          contextMenu(
            e.clientX,
            e.clientY,
            [
              {
                label: `Monocord v${Api.version} \u2014 ${enabled} plugins active`,
                action: () => {
                }
              },
              {
                label: "Copy plugin list",
                action: () => {
                  void navigator.clipboard.writeText(Api.Plugins.names().join("\n")).then(() => toast("Plugin list copied")).catch(() => {
                  });
                }
              },
              {
                label: "Reload Discord",
                action: () => location.reload()
              }
            ]
          );
        });
        bar.append(btn);
      });
      this.__stop = stop;
    },
    stop() {
      this.__stop?.();
    }
  };
  var ThemeAttributes = {
    name: "ThemeAttributes",
    description: "Add data attributes to elements for custom theming.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const stop = observeAdded('[id^="chat-messages-"]', (node) => {
        const m = /chat-messages-(\d+)-(\d+)/.exec(node.id);
        if (!m) return;
        const el2 = node;
        if (!el2.dataset.mcChannel) el2.dataset.mcChannel = m[1];
        if (!el2.dataset.mcMessage) el2.dataset.mcMessage = m[2];
      });
      this.__stop = stop;
    },
    stop() {
      this.__stop?.();
    }
  };
  function registerSystem3() {
    for (const plugin of [
      AlwaysTrust,
      DisableDeepLinks,
      NoDevtoolsWarning,
      NoOnboardingDelay,
      KeepCurrentChannel,
      CustomIdle,
      DisableCallIdle,
      CallTimer,
      StreamerModeOnStream,
      OpenInApp,
      OnePingPerDM,
      VolumeBooster,
      iLoveSpam,
      RelationshipNotifier,
      SuperReactionTweaks,
      UnlockedAvatarZoom,
      USRBG,
      Dearrow,
      BetterYouTubeEmbeds,
      FixSpotifyEmbeds,
      AnonymiseFileNames,
      ReactErrorDecoder,
      F8Break,
      Experiments,
      MonocordToolbox,
      ThemeAttributes
    ]) {
      try {
        Api.Plugins.register(plugin);
      } catch (e) {
        log12.error(`failed to register ${plugin.name}`, e);
      }
    }
  }

  // src/plugins/ux-extras.ts
  var log13 = Logger.get("ux-extras");
  var CharacterCounter = {
    name: "CharacterCounter",
    description: "Shows the character count of your message draft.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const cleanups = [];
      const attach = (container) => {
        const editable = container.querySelector(
          '[contenteditable="true"], textarea'
        ) ?? container;
        if (editable.dataset?.mcCounter) return;
        editable.dataset.mcCounter = "1";
        const counter = el(
          "div",
          {
            position: "absolute",
            bottom: "6px",
            right: "12px",
            fontSize: "11px",
            color: "var(--text-muted, #949ba4)",
            pointerEvents: "none",
            zIndex: "5",
            fontVariantNumeric: "tabular-nums"
          }
        );
        const parent = container instanceof HTMLElement ? container : editable;
        const prevPosition = parent.style.position;
        parent.style.position = "relative";
        parent.append(counter);
        const update = () => {
          const text = editable instanceof HTMLTextAreaElement ? editable.value : editable.textContent ?? "";
          counter.textContent = String(text.length);
          counter.style.color = text.length > 1900 ? "var(--status-danger, #f23f43)" : "var(--text-muted, #949ba4)";
        };
        editable.addEventListener("input", update);
        update();
        cleanups.push(() => {
          editable.removeEventListener("input", update);
          counter.remove();
          parent.style.position = prevPosition;
        });
      };
      const stopObserve = observeAdded('[class*="channelTextArea"]', attach);
      cleanups.push(stopObserve);
      this.__cleanup = () => cleanups.forEach((c) => c());
    },
    stop() {
      this.__cleanup?.();
    }
  };
  var TRACKING_PARAMS = /^(utm_|fbclid|gclid|dclid|gclsrc|msclkid|mc_cid|mc_eid|_hsenc|_hsmi|vero_id|wickedid|ttclid|twclid|li_fat_id|igshid|spm|scm)/i;
  var ClearURLs = {
    name: "ClearURLs",
    description: "Removes tracking parameters from links you click.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const handler = (ev) => {
        const t = ev.target;
        if (!(t instanceof Element)) return;
        const a = t.closest("a[href]");
        if (!(a instanceof HTMLAnchorElement)) return;
        try {
          const url = new URL(a.href);
          let changed = false;
          for (const key of [...url.searchParams.keys()]) {
            if (TRACKING_PARAMS.test(key)) {
              url.searchParams.delete(key);
              changed = true;
            }
          }
          if (changed) a.href = url.toString();
        } catch {
        }
      };
      document.addEventListener("click", handler, true);
      document.addEventListener("auxclick", handler, true);
      this.__off = () => {
        document.removeEventListener("click", handler, true);
        document.removeEventListener("auxclick", handler, true);
      };
    },
    stop() {
      this.__off?.();
    }
  };
  var MEDIA_HOST = /media\.discordapp\.net|images-ext|cdn\.discordapp\.com\/attachments|media\.discordapp\.com/i;
  function isMessageImage(img) {
    if (!MEDIA_HOST.test(img.currentSrc || img.src)) return false;
    return img.naturalWidth > 96 && img.naturalHeight > 96;
  }
  var ImageZoom = {
    name: "ImageZoom",
    description: "Click an image in chat to view it full size.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const open = (src) => {
        const overlay = el(
          "div",
          {
            position: "fixed",
            inset: "0",
            zIndex: "100001",
            background: "rgba(0,0,0,.85)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            cursor: "zoom-out"
          }
        );
        const img = document.createElement("img");
        img.src = src;
        img.style.cssText = "max-width:92vw;max-height:92vh;object-fit:contain;border-radius:8px;";
        overlay.append(img);
        const close = () => overlay.remove();
        overlay.addEventListener("click", close);
        document.addEventListener("keydown", function onKey(e) {
          if (e.key === "Escape") {
            close();
            document.removeEventListener("keydown", onKey);
          }
        });
        document.body.append(overlay);
      };
      const off = on(document, "click", "img", (node, ev) => {
        const img = node;
        if (img.closest('[class*="imageZoom"], [class*="modal"]')) return;
        if (!isMessageImage(img)) return;
        ev.preventDefault();
        ev.stopPropagation();
        open(img.currentSrc || img.src);
      });
      this.__off = off;
    },
    stop() {
      this.__off?.();
    }
  };
  var ReverseImageSearch = {
    name: "ReverseImageSearch",
    description: "Right-click an image to reverse image search it.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const off = on(document, "contextmenu", "img", (node, ev) => {
        const img = node;
        if (!isMessageImage(img)) return;
        ev.preventDefault();
        ev.stopPropagation();
        const src = img.currentSrc || img.src;
        const menu = el(
          "div",
          {
            position: "fixed",
            left: `${ev.clientX}px`,
            top: `${ev.clientY}px`,
            zIndex: "100001",
            background: "var(--background-floating, #111214)",
            border: "1px solid var(--background-modifier-accent, #3f4147)",
            borderRadius: "8px",
            padding: "4px",
            minWidth: "180px",
            boxShadow: "0 8px 24px rgba(0,0,0,.4)"
          }
        );
        const close = () => menu.remove();
        const item = (label, url) => {
          const b = el(
            "button",
            {
              display: "block",
              width: "100%",
              textAlign: "left",
              background: "none",
              border: "none",
              color: "var(--text-normal, #dbdee1)",
              padding: "8px 12px",
              fontSize: "13px",
              cursor: "pointer",
              borderRadius: "4px"
            },
            label
          );
          b.addEventListener("mouseenter", () => b.style.background = "var(--background-modifier-hover, rgba(78,80,88,.3))");
          b.addEventListener("mouseleave", () => b.style.background = "none");
          b.addEventListener("click", () => {
            window.open(url, "_blank");
            close();
          });
          return b;
        };
        menu.append(
          item(
            "Search with Google Lens",
            `https://lens.google.com/uploadbyurl?url=${encodeURIComponent(src)}`
          ),
          item(
            "Search with SauceNAO",
            `https://saucenao.com/search.php?url=${encodeURIComponent(src)}`
          )
        );
        document.body.append(menu);
        setTimeout(() => {
          document.addEventListener("click", close, { once: true });
          document.addEventListener("contextmenu", close, { once: true });
        }, 0);
      });
      this.__off = off;
    },
    stop() {
      this.__off?.();
    }
  };
  var PictureInPicture = {
    name: "PictureInPicture",
    description: "Adds a picture-in-picture button to videos.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const stop = observeAdded("video", (video) => {
        if (!(video instanceof HTMLVideoElement)) return;
        if (video.dataset.mcPip) return;
        video.dataset.mcPip = "1";
        const wrap2 = video.parentElement;
        if (!wrap2) return;
        const prevPosition = wrap2.style.position;
        wrap2.style.position = "relative";
        const btn = el(
          "button",
          {
            position: "absolute",
            bottom: "8px",
            right: "8px",
            zIndex: "5",
            background: "rgba(0,0,0,.6)",
            color: "#fff",
            border: "none",
            borderRadius: "4px",
            padding: "4px 8px",
            fontSize: "11px",
            cursor: "pointer",
            opacity: "0",
            transition: "opacity .15s"
          },
          "PiP"
        );
        wrap2.addEventListener("mouseenter", () => btn.style.opacity = "1");
        wrap2.addEventListener("mouseleave", () => btn.style.opacity = "0");
        btn.addEventListener("click", (e) => {
          e.stopPropagation();
          video.requestPictureInPicture?.().catch(() => toast("Picture-in-picture unavailable"));
        });
        wrap2.append(btn);
        video.__mcCleanup = () => {
          btn.remove();
          wrap2.style.position = prevPosition;
        };
      });
      this.__stop = stop;
    },
    stop() {
      this.__stop?.();
    }
  };
  var Oneko = {
    name: "Oneko",
    description: "A cat emoji follows your cursor around Discord.",
    authors: ["Monocord"],
    version: "0.1.0",
    settings: { speed: 300 },
    settingsSchema: {
      speed: {
        type: "number",
        label: "Speed (ms per step)",
        description: "Lower = faster cat."
      }
    },
    start() {
      const cat = el(
        "div",
        {
          position: "fixed",
          zIndex: "99999",
          fontSize: "42px",
          pointerEvents: "none",
          left: "0",
          top: "0",
          transition: "transform .45s ease-out",
          willChange: "transform"
        },
        "\u{1F431}"
      );
      document.body.append(cat);
      let x = 0;
      let y = 0;
      let cx = 0;
      let cy = 0;
      const onMove = (e) => {
        x = e.clientX;
        y = e.clientY;
      };
      document.addEventListener("mousemove", onMove);
      const interval = setInterval(() => {
        cx += (x - cx) * 0.18;
        cy += (y - cy) * 0.18;
        cat.style.transform = `translate(${cx - 21}px, ${cy - 21}px)`;
      }, 50);
      this.__cleanup = () => {
        clearInterval(interval);
        document.removeEventListener("mousemove", onMove);
        cat.remove();
      };
    },
    stop() {
      this.__cleanup?.();
    }
  };
  var VoiceChatDoubleClick = {
    name: "VoiceChatDoubleClick",
    description: "Double-click a voice channel to join it.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const off = on(document, "dblclick", '[class*="container"], li, [role="treeitem"]', (node) => {
        const row = node;
        const joinBtn = row.querySelector(
          'button[aria-label*="oice"], [class*="join"], [class*="connect"]'
        );
        if (joinBtn) {
          joinBtn.click();
          return;
        }
        const icon = row.querySelector('svg[aria-label*="oice"], [class*="icon"]');
        if (icon) icon.click();
      });
      this.__off = off;
    },
    stop() {
      this.__off?.();
    }
  };
  var QUALITY_RE = /([?&])quality=[^&]*/;
  var FixImagesQuality = {
    name: "FixImagesQuality",
    description: "Loads chat images at lossless quality instead of compressed.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const fix = (img) => {
        if (img.dataset.mcQuality) return;
        const src = img.currentSrc || img.src;
        if (!MEDIA_HOST.test(src)) return;
        img.dataset.mcQuality = "1";
        try {
          const url = new URL(src);
          if (!QUALITY_RE.test(url.search)) {
            url.searchParams.set("quality", "lossless");
          } else {
            url.search = url.search.replace(QUALITY_RE, "$1quality=lossless");
          }
          const next = url.toString();
          if (next !== src) img.src = next;
        } catch {
        }
      };
      const stop = observeAdded("img", (node) => {
        if (node instanceof HTMLImageElement) fix(node);
      });
      const interval = setInterval(() => {
        document.querySelectorAll("img").forEach((img) => {
          if (!img.dataset.mcQuality) fix(img);
        });
      }, 3e3);
      this.__cleanup = () => {
        stop();
        clearInterval(interval);
      };
    },
    stop() {
      this.__cleanup?.();
    }
  };
  var SendTimestamps = {
    name: "SendTimestamps",
    description: "Insert Discord timestamps (<t:\u2026:F>) into your message.",
    authors: ["Monocord"],
    version: "0.1.0",
    settings: { style: "F" },
    settingsSchema: {
      style: {
        type: "select",
        label: "Timestamp format",
        options: [
          ["t", "Short time (16:20)"],
          ["T", "Long time (16:20:30)"],
          ["d", "Short date (20/04/2026)"],
          ["D", "Long date (April 20, 2026)"],
          ["f", "Short date+time"],
          ["F", "Full date+time"],
          ["R", "Relative (2 hours ago)"]
        ].map(([value, label]) => ({ value, label }))
      }
    },
    start() {
      const cleanups = [];
      const attach = (container) => {
        if (container.querySelector("[data-mc-ts]")) return;
        const btn = el(
          "button",
          {
            position: "absolute",
            bottom: "6px",
            right: "34px",
            zIndex: "5",
            background: "var(--background-modifier-accent, #3f4147)",
            color: "var(--text-normal, #dbdee1)",
            border: "none",
            borderRadius: "4px",
            padding: "2px 8px",
            fontSize: "11px",
            cursor: "pointer"
          },
          "TS"
        );
        btn.dataset.mcTs = "1";
        const parent = container;
        const prevPosition = parent.style.position;
        parent.style.position = "relative";
        parent.append(btn);
        btn.addEventListener("click", () => {
          void Api.Settings.get(
            "plugins.settings.SendTimestamps.style",
            "F"
          ).then((s) => insertTimestamp(s ?? "F"));
        });
        cleanups.push(() => {
          btn.remove();
          parent.style.position = prevPosition;
        });
      };
      const stopObserve = observeAdded('[class*="channelTextArea"]', attach);
      cleanups.push(stopObserve);
      this.__cleanup = () => cleanups.forEach((c) => c());
    },
    stop() {
      this.__cleanup?.();
    }
  };
  function insertTimestamp(style) {
    const editable = document.querySelector(
      '[class*="channelTextArea"] [contenteditable="true"], [class*="channelTextArea"] textarea'
    );
    if (!editable) {
      toast("Focus the message box first");
      return;
    }
    editable.focus();
    const ts = Math.floor(Date.now() / 1e3);
    const text = `<t:${ts}:${style}>`;
    if (editable instanceof HTMLTextAreaElement) {
      const s = editable.selectionStart ?? editable.value.length;
      editable.value = editable.value.slice(0, s) + text + editable.value.slice(s);
      editable.dispatchEvent(new Event("input", { bubbles: true }));
      editable.selectionStart = editable.selectionEnd = s + text.length;
      return;
    }
    document.execCommand("insertText", false, text);
  }
  var TEXT_EXT = /\.(txt|md|markdown|json|log|csv|xml|ya?ml|ini|cfg|env|py|js|ts|html?|css|rs|go|java|c|cpp|h|sh|bat|ps1)$/i;
  var CopyFileContents = {
    name: "CopyFileContents",
    description: "Right-click a text-file attachment to copy its contents.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const off = on(
        document,
        "contextmenu",
        'a[href*="cdn.discordapp.com/attachments"], a[href*="media.discordapp.net/attachments"]',
        (node, ev) => {
          const a = node;
          const href = a.href;
          if (!TEXT_EXT.test(new URL(href).pathname)) return;
          ev.preventDefault();
          ev.stopPropagation();
          void (async () => {
            try {
              const res = await fetch(href);
              if (!res.ok) throw new Error(String(res.status));
              const text = await res.text();
              await navigator.clipboard.writeText(text);
              toast("File contents copied");
            } catch (e) {
              log13.warn("copy file contents failed", e);
              toast("Failed to copy file");
            }
          })();
        }
      );
      this.__off = off;
    },
    stop() {
      this.__off?.();
    }
  };
  var THEME_VARS = {
    brand: "--brand-500",
    brandHover: "--brand-560",
    background: "--background-primary",
    secondary: "--background-secondary",
    tertiary: "--background-tertiary",
    floating: "--background-floating",
    text: "--text-normal",
    muted: "--text-muted",
    success: "--status-positive",
    danger: "--status-danger",
    warning: "--status-warning"
  };
  var ClientTheme = {
    name: "ClientTheme",
    description: "Override Discord's color theme via settings.",
    authors: ["Monocord"],
    version: "0.1.0",
    settings: {
      brand: "",
      brandHover: "",
      background: "",
      secondary: "",
      tertiary: "",
      floating: "",
      text: "",
      muted: "",
      success: "",
      danger: "",
      warning: ""
    },
    settingsSchema: Object.fromEntries(
      [
        ["brand", "Brand color"],
        ["brandHover", "Brand hover"],
        ["background", "Background"],
        ["secondary", "Secondary background"],
        ["tertiary", "Tertiary background"],
        ["floating", "Floating background"],
        ["text", "Text color"],
        ["muted", "Muted text"],
        ["success", "Success"],
        ["danger", "Danger"],
        ["warning", "Warning"]
      ].map(([key, label]) => [
        key,
        {
          type: "string",
          label,
          description: "Hex or rgb() color, e.g. #5865f2. Empty = default.",
          placeholder: "#5865f2"
        }
      ])
    ),
    async start() {
      const apply2 = async () => {
        const parts = [":root {"];
        for (const [key, cssVar] of Object.entries(THEME_VARS)) {
          const value = await Api.Settings.get(
            `plugins.settings.ClientTheme.${key}`
          );
          if (value && /^#[0-9a-fA-F]{3,8}$|^rgba?\(/.test(value.trim())) {
            parts.push(`  ${cssVar}: ${value.trim()};`);
          }
        }
        parts.push("}");
        Api.Themes.injectCss("clienttheme", parts.join("\n"));
      };
      Api.Events.on("settings.changed", (payload) => {
        const path = payload?.path;
        if (typeof path === "string" && path.startsWith("plugins.settings.ClientTheme.")) {
          void apply2();
        }
      });
      await apply2();
    },
    stop() {
      Api.Themes.removeCss("clienttheme");
    }
  };
  var BlurNSFW = cssPlugin(
    "BlurNSFW",
    "Blur images inside NSFW-flagged channels until hovered.",
    `
    [class*="nsfw"] img,
    [class*="nsfw"] video {
        filter: blur(18px) saturate(.7) !important;
        transition: filter .2s ease !important;
    }
    [class*="nsfw"] img:hover,
    [class*="nsfw"] video:hover {
        filter: none !important;
    }
    `
  );
  var AlwaysAnimate = cssPlugin(
    "AlwaysAnimate",
    "Force all CSS animations to keep playing.",
    `
    *, *::before, *::after {
        animation-play-state: running !important;
    }
    `
  );
  var BiggerStreamPreview = cssPlugin(
    "BiggerStreamPreview",
    "Enlarge the stream preview modal.",
    `
    [class*="streamPreview"],
    [class*="previewModal"] [class*="video"],
    [class*="previewModal"] video {
        max-width: 82vw !important;
        max-height: 82vh !important;
    }
    `
  );
  function registerUxExtras() {
    for (const plugin of [
      CharacterCounter,
      ClearURLs,
      ImageZoom,
      ReverseImageSearch,
      PictureInPicture,
      Oneko,
      VoiceChatDoubleClick,
      FixImagesQuality,
      SendTimestamps,
      CopyFileContents,
      ClientTheme,
      BlurNSFW,
      AlwaysAnimate,
      BiggerStreamPreview
    ]) {
      try {
        Api.Plugins.register(plugin);
      } catch (e) {
        log13.error(`failed to register ${plugin.name}`, e);
      }
    }
  }

  // src/plugins/vencord-late.ts
  var log14 = Logger.get("vencord-late");
  function storeMethod(name, method) {
    const store = findStoreByName(name);
    const fn = store?.[method];
    return typeof fn === "function" ? fn : null;
  }
  var PRESET_QUOTES = [
    "Did you know? You can press Ctrl+K to quick switch channels.",
    "Pro tip: Shift+Click a message to select multiple messages.",
    "You can reply to a message by swiping right on mobile.",
    "Discord was originally called 'Scratchpad'.",
    "You can pin up to 50 messages per channel.",
    "Hold Alt while dragging to resize image previews.",
    "You can use ||spoiler tags|| to hide text until hovered.",
    "Pressing ? shows you all available keyboard shortcuts.",
    "You can edit your own messages for up to a year.",
    "Nitro boosts level up your server's emoji slots and quality.",
    "You can react with any emoji, including server-specific ones.",
    "Drag and drop files anywhere in a channel to upload them.",
    "You can create a poll using the built-in poll feature.",
    "Typing / in chat opens the slash command menu.",
    "You can mute a channel without leaving it.",
    "Server discovery lets you find communities by topic.",
    "You can mark a server as a home channel for quick access.",
    "Push-to-talk can be bound to any key or mouse button.",
    "You can stream your screen in any voice channel.",
    "Custom statuses let you show what you're up to."
  ];
  var LoadingQuotes = {
    name: "LoadingQuotes",
    description: "Replace Discord's loading quotes with your own.",
    authors: ["Monocord"],
    version: "0.1.0",
    settings: { quotes: "", usePreset: true, useDiscord: false },
    settingsSchema: {
      usePreset: {
        type: "boolean",
        label: "Include preset quotes",
        description: "Use Monocord's built-in quote list."
      },
      useDiscord: {
        type: "boolean",
        label: "Keep Discord quotes",
        description: "Also show Discord's original loading quotes."
      },
      quotes: {
        type: "textarea",
        label: "Custom quotes",
        description: "One quote per line."
      }
    },
    async start() {
      const readQuotes = async () => {
        const s = await Promise.all([
          Api.Settings.get("plugins.settings.LoadingQuotes.usePreset", true),
          Api.Settings.get("plugins.settings.LoadingQuotes.useDiscord", false),
          Api.Settings.get("plugins.settings.LoadingQuotes.quotes", "")
        ]);
        const out = [];
        if (s[1]) out.push("__DISCORD__");
        if (s[0]) out.push(...PRESET_QUOTES);
        out.push(
          ...(s[2] ?? "").split("\n").map((q) => q.trim()).filter(Boolean)
        );
        return out;
      };
      let quotes = await readQuotes();
      Api.Events.on("settings.changed", (payload) => {
        const path = payload?.path;
        if (typeof path === "string" && path.startsWith("plugins.settings.LoadingQuotes")) {
          void readQuotes().then((q) => quotes = q);
        }
      });
      const pick = () => quotes.length ? quotes[Math.floor(Math.random() * quotes.length)] : null;
      observeAdded('[class*="quote" i]', (node) => {
        const html = node;
        if (html.dataset.mcQuote) return;
        if (html.closest('[class*="message"]')) return;
        const replacement = pick();
        if (!replacement || replacement === "__DISCORD__") return;
        html.dataset.mcQuote = "1";
        const original = html.textContent ?? "";
        html.dataset.mcQuoteOrig = original;
        html.textContent = replacement;
      });
    }
  };
  var MessageLinkEmbeds = {
    name: "MessageLinkEmbeds",
    description: "Preview messages linked with discord.com/channels URLs (uses already-loaded messages only).",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const getMessage2 = storeMethod(
        "MessageStore",
        "getMessage"
      );
      if (!getMessage2) {
        log14.warn("MessageStore.getMessage not found \u2014 plugin inactive");
        return;
      }
      const cache = /* @__PURE__ */ new Map();
      const linkRe = /https?:\/\/(?:\w+\.)?discord(?:app)?\.com\/channels\/(?:\d{17,20}|@me)\/(\d{17,20})\/(\d{17,20})/;
      observeAdded("a[href*='discord.com/channels/'], a[href*='discordapp.com/channels/']", (a) => {
        const html = a;
        const msgRoot = html.closest('[id^="chat-messages-"]');
        if (!msgRoot || msgRoot.querySelector("[data-mc-msglink]")) return;
        const m = linkRe.exec(html.href);
        if (!m) return;
        const [, channelId, messageId] = m;
        if (!channelId || !messageId) return;
        const key = `${channelId}:${messageId}`;
        if (cache.get(key)) return;
        let msg = null;
        try {
          msg = getMessage2(channelId, messageId);
        } catch {
          msg = null;
        }
        cache.set(key, !!msg);
        if (!msg) return;
        const author = isObj(msg.author) ? msg.author : {};
        const authorName = String(author.global_name ?? author.username ?? "Unknown");
        const content = String(msg.content ?? "").slice(0, 140);
        const card = el(
          "div",
          {
            marginTop: "4px",
            padding: "6px 10px",
            borderLeft: "3px solid var(--brand-500,#5865f2)",
            background: "var(--background-secondary,#2b2d31)",
            borderRadius: "4px",
            fontSize: "13px"
          }
        );
        card.dataset.mcMsglink = "1";
        card.append(
          el(
            "div",
            { fontWeight: "600", fontSize: "12px", color: "var(--text-muted,#949ba4)" },
            authorName
          ),
          el("div", { marginTop: "2px" }, content || "(no text content)")
        );
        const contentEl = html.closest('[class*="content"], [class*="messageContent"]') ?? msgRoot;
        contentEl.append(card);
      });
    }
  };
  var DontRoundMyTimestamps = {
    name: "DontRoundMyTimestamps",
    description: "Round relative timestamps down (7.6y \u2192 7y instead of 8y).",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const candidates = Api.Webpack.findAll(() => true);
      for (const exp of candidates) {
        const objs = [exp, isObj(exp) ? exp.default : null].filter(isObj);
        for (const o of objs) {
          const rtr = o.relativeTimeRounding;
          if (typeof rtr === "function") {
            try {
              rtr(Math.floor);
              log14.info("moment.relativeTimeRounding \u2192 floor");
              return;
            } catch (e) {
              log14.debug("relativeTimeRounding call failed", e);
            }
          }
        }
      }
      log14.warn("moment.relativeTimeRounding not found \u2014 plugin inactive");
    }
  };
  var ReadAllNotificationsButton = {
    name: "ReadAllNotificationsButton",
    description: "Mark all server notifications as read with one click.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const dispatcher2 = findDispatcher();
      const getGuilds = storeMethod("GuildStore", "getGuilds");
      const getChannels = storeMethod("GuildChannelStore", "getChannels");
      const threads = storeMethod("ActiveJoinedThreadsStore", "getActiveJoinedThreadsForGuild");
      const hasUnread = storeMethod("ReadStateStore", "hasUnread");
      const lastMessageId = storeMethod("ReadStateStore", "lastMessageId");
      if (!dispatcher2?.dispatch || !getGuilds || !getChannels || !hasUnread || !lastMessageId) {
        log14.warn("required stores/dispatcher missing \u2014 plugin inactive");
        return;
      }
      const channelIdsOf = (guildId) => {
        const out = [];
        try {
          const chans = getChannels(guildId);
          for (const key of ["SELECTABLE", "VOCAL"]) {
            const list = chans[key];
            if (!Array.isArray(list)) continue;
            for (const item of list) {
              const ch = isObj(item) && isObj(item.channel) ? item.channel : item;
              if (isObj(ch) && typeof ch.id === "string") out.push(ch.id);
            }
          }
          if (threads) {
            const t = threads(guildId);
            if (isObj(t)) {
              for (const v of Object.values(t)) {
                if (Array.isArray(v)) {
                  for (const th of v) {
                    if (isObj(th) && typeof th.id === "string") out.push(th.id);
                  }
                } else if (isObj(v) && typeof v.id === "string") {
                  out.push(v.id);
                }
              }
            }
          }
        } catch (e) {
          log14.debug("channel enumeration failed", e);
        }
        return out;
      };
      const stop = observeAdded('[class*="guilds"]', (node) => {
        const html = node;
        if (html.querySelector("[data-mc-readall]")) return;
        const btn = el(
          "button",
          {
            margin: "8px auto 0",
            display: "block",
            padding: "4px 12px",
            fontSize: "12px",
            borderRadius: "4px",
            border: "1px solid var(--background-modifier-accent,#3f4147)",
            background: "var(--background-secondary,#2b2d31)",
            color: "var(--text-normal,#dbdee1)",
            cursor: "pointer"
          },
          "Read All"
        );
        btn.dataset.mcReadall = "1";
        btn.addEventListener("click", () => {
          const channels = [];
          try {
            for (const g of Object.values(getGuilds())) {
              if (!isObj(g) || typeof g.id !== "string") continue;
              for (const id of channelIdsOf(g.id)) {
                if (!hasUnread(id)) continue;
                channels.push({
                  channelId: id,
                  messageId: lastMessageId(id),
                  readStateType: 0
                });
              }
            }
          } catch (e) {
            log14.warn("read-all failed", e);
            return;
          }
          if (channels.length) {
            dispatcher2.dispatch({
              type: "BULK_ACK",
              context: "APP",
              channels
            });
          }
        });
        html.prepend(btn);
      });
      this.__cleanup = stop;
    },
    stop() {
      document.querySelectorAll("[data-mc-readall]").forEach((n) => n.remove());
      this.__cleanup?.();
    }
  };
  var ShowTimeoutDuration = {
    name: "ShowTimeoutDuration",
    description: "Show how much longer a member timeout lasts.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const getMember = storeMethod(
        "GuildMemberStore",
        "getMember"
      );
      if (!getMember) {
        log14.warn("GuildMemberStore.getMember not found \u2014 plugin inactive");
        return;
      }
      const guildFromUrl = () => /\/channels\/(\d{15,25})\//.exec(location.pathname)?.[1] ?? null;
      const userFromRow = (row) => {
        const img = row.querySelector('img[src*="/avatars/"], img[src*="/guilds/"]');
        const m = img ? /\/(\d{15,25})\//.exec(img.src) : null;
        return m?.[1] ?? null;
      };
      const fmt = (ms) => {
        const mins = Math.floor(ms / 6e4);
        if (mins < 60) return `${mins}m`;
        const hrs = Math.floor(mins / 60);
        if (hrs < 24) return `${hrs}h`;
        return `${Math.floor(hrs / 24)}d`;
      };
      observeAdded('[class*="timeout" i], [class*="communicationDisabled" i]', (node) => {
        const html = node;
        if (html.dataset.mcTimeout) return;
        const row = html.closest('[class*="member"], li, [role="listitem"]') ?? html.parentElement;
        if (!row) return;
        const uid = userFromRow(row);
        const gid = guildFromUrl();
        if (!uid || !gid) return;
        let until;
        try {
          until = getMember(gid, uid)?.communicationDisabledUntil;
        } catch {
          return;
        }
        const ts = typeof until === "string" ? Date.parse(until) : Number(until);
        if (!ts || ts <= Date.now()) return;
        html.dataset.mcTimeout = "1";
        const label = el(
          "span",
          {
            marginLeft: "4px",
            fontSize: "11px",
            color: "var(--text-muted,#949ba4)"
          },
          `(${fmt(ts - Date.now())})`
        );
        html.append(label);
      });
    }
  };
  var SortFriendRequests = {
    name: "SortFriendRequests",
    description: "Sort friend requests by date of receipt (newest first).",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const getSince = storeMethod("RelationshipStore", "getSince");
      if (!getSince) {
        log14.warn("RelationshipStore.getSince not found \u2014 plugin inactive");
        return;
      }
      let timer2;
      const resort = () => {
        const containers = document.querySelectorAll(
          '[class*="incomingRequests"], [class*="friendRequests"], [class*="requests"]'
        );
        for (const c of containers) {
          const items = Array.from(c.querySelectorAll(":scope > *"));
          const keyed = items.map((item) => {
            const img = item.querySelector(
              'img[src*="/avatars/"], img[src*="/embed/avatars/"]'
            );
            const uid = img ? /(\d{15,25})/.exec(img.src)?.[1] : null;
            if (!uid) return null;
            let since = 0;
            try {
              since = Number(getSince(uid)) || 0;
            } catch {
              return null;
            }
            return { item, since };
          }).filter((x) => !!x);
          if (keyed.length < 2) continue;
          keyed.sort((a, b) => b.since - a.since);
          for (const { item } of keyed) c.append(item);
        }
      };
      const schedule = () => {
        if (timer2) clearTimeout(timer2);
        timer2 = setTimeout(resort, 400);
      };
      const stopObserve = observeAdded('[class*="friend" i], [class*="request" i]', schedule);
      const interval = setInterval(resort, 1e4);
      this.__cleanup = () => {
        stopObserve();
        clearInterval(interval);
        if (timer2) clearTimeout(timer2);
      };
    },
    stop() {
      this.__cleanup?.();
    }
  };
  var MentionAvatars = {
    name: "MentionAvatars",
    description: "Show user avatars inside @mentions.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const getUser = storeMethod("UserStore", "getUser");
      if (!getUser) {
        log14.warn("UserStore.getUser not found \u2014 plugin inactive");
        return;
      }
      const avatarUrl = (u) => {
        const id = String(u.id ?? "");
        const avatar = String(u.avatar ?? "");
        if (!id || !avatar) return null;
        const ext = avatar.startsWith("a_") ? "gif" : "png";
        return `https://cdn.discordapp.com/avatars/${id}/${avatar}.${ext}?size=32`;
      };
      observeAdded('[class*="mention"]', (node) => {
        const html = node;
        if (html.dataset.mcMentionAvatar || html.querySelector("img[data-mc-ma]")) return;
        const raw = html.getAttribute("data-id") ?? html.id ?? /users\/(\d{15,25})/.exec(html.innerHTML)?.[1] ?? null;
        const uid = raw ? /(\d{15,25})/.exec(raw)?.[1] : null;
        if (!uid) return;
        let user = null;
        try {
          user = getUser(uid);
        } catch {
          user = null;
        }
        const url = user ? avatarUrl(user) : null;
        if (!url) return;
        html.dataset.mcMentionAvatar = "1";
        const img = document.createElement("img");
        img.dataset.mcMa = "1";
        img.src = url;
        img.alt = "";
        Object.assign(img.style, {
          width: "16px",
          height: "16px",
          borderRadius: "50%",
          marginRight: "3px",
          verticalAlign: "text-bottom"
        });
        html.prepend(img);
      });
    }
  };
  var UserMessagesPronouns = {
    name: "UserMessagesPronouns",
    description: "Show users' profile pronouns next to their names in chat.",
    authors: ["Monocord"],
    version: "0.1.0",
    settings: { lowercase: false },
    settingsSchema: {
      lowercase: {
        type: "boolean",
        label: "Lowercase pronouns"
      }
    },
    start() {
      const profile = findStoreByName("UserProfileStore");
      const getProfile = profile?.getUserProfile;
      if (!getProfile) {
        log14.warn("UserProfileStore.getUserProfile not found \u2014 plugin inactive");
        return;
      }
      const cache = /* @__PURE__ */ new Map();
      const pronounsFor = async (uid) => {
        if (cache.has(uid)) return cache.get(uid) ?? null;
        let p = null;
        try {
          const prof = getProfile(uid);
          const raw = prof?.pronouns;
          if (typeof raw === "string" && raw.trim()) {
            p = raw.trim().replace(/\n+/g, " ");
            if (await Api.Settings.get(
              "plugins.settings.UserMessagesPronouns.lowercase",
              false
            ) === true) {
              p = p.toLowerCase();
            }
          }
        } catch {
          p = null;
        }
        cache.set(uid, p);
        return p;
      };
      observeAdded('[id^="chat-messages-"]', (node) => {
        const msg = node;
        if (msg.dataset.mcPronouns) return;
        const avatar = msg.querySelector(
          'img[src*="/avatars/"]'
        );
        const uid = avatar ? /(\d{15,25})/.exec(avatar.src)?.[1] : null;
        const nameEl = msg.querySelector('[class*="username"]');
        if (!uid || !nameEl || nameEl.querySelector("[data-mc-pronouns]")) return;
        msg.dataset.mcPronouns = "1";
        void pronounsFor(uid).then((p) => {
          if (!p) return;
          const tag = el(
            "span",
            {
              marginLeft: "4px",
              fontSize: "12px",
              fontWeight: "400",
              color: "var(--text-muted,#949ba4)"
            },
            `(${p})`
          );
          tag.dataset.mcPronouns = "1";
          nameEl.append(tag);
        });
      });
    }
  };
  var PLUGINS = [
    LoadingQuotes,
    MessageLinkEmbeds,
    DontRoundMyTimestamps,
    ReadAllNotificationsButton,
    ShowTimeoutDuration,
    SortFriendRequests,
    MentionAvatars,
    UserMessagesPronouns
  ];
  function registerVencordLate() {
    for (const p of PLUGINS) {
      try {
        Api.Plugins.register(p);
      } catch (e) {
        log14.warn(`register ${p.name} failed`, e);
      }
    }
  }

  // src/plugins/vencord-last.ts
  var log15 = Logger.get("vencord-last");
  function storeMethod2(name, method) {
    const store = findStoreByName(name);
    const fn = store?.[method];
    return typeof fn === "function" ? fn : null;
  }
  var ConsoleShortcuts = {
    name: "ConsoleShortcuts",
    description: "Exposes `window.mc` with API, stores, and webpack helpers for console debugging.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const w = window;
      if (w.mc) return;
      w.mc = {
        Api,
        Plugins: Api.Plugins,
        Settings: Api.Settings,
        Events: Api.Events,
        Themes: Api.Themes,
        version: Api.version,
        reload: () => location.reload(),
        plugins: () => Api.Plugins.names(),
        enable: (n) => Api.Plugins.enable(n),
        disable: (n) => Api.Plugins.disable(n),
        stores: () => {
          const out = {};
          for (const name of [
            "UserStore",
            "GuildStore",
            "ChannelStore",
            "MessageStore",
            "ReadStateStore",
            "GuildMemberStore",
            "PresenceStore",
            "RelationshipStore",
            "EmojiStore",
            "UserProfileStore",
            "SelectedChannelStore",
            "PermissionStore"
          ]) {
            const s = Api.Webpack.findStore(name);
            if (s) out[name] = s;
          }
          return out;
        },
        find: (f) => Api.Webpack.find(f),
        findAll: (f) => Api.Webpack.findAll(f),
        findByProps: (...p) => Api.Webpack.findByProps(...p),
        findAllByProps: (...p) => Api.Webpack.findAllByProps(...p),
        findStore: (n) => Api.Webpack.findStore(n),
        channelId: () => /\/channels\/(?:@me|\d+)\/(\d+)/.exec(location.pathname)?.[1] ?? null,
        guildId: () => /\/channels\/(\d{15,25})\//.exec(location.pathname)?.[1] ?? null,
        dispatch: (a) => {
          const d = findDispatcher();
          d?.dispatch(a);
        },
        shortcutList: () => Object.keys(w.mc)
      };
      log15.info("window.mc shortcuts installed \u2014 run `mc.shortcutList()`");
    }
  };
  var MusicRichPresence = {
    name: "MusicRichPresence",
    description: "Show your current Last.fm track as Discord rich presence. Requires a Last.fm API key.",
    authors: ["Monocord"],
    version: "0.1.0",
    settings: {
      apiKey: "",
      username: "",
      showAlbum: true,
      mode: "listening"
    },
    settingsSchema: {
      apiKey: {
        type: "string",
        label: "Last.fm API key",
        description: "Get one free at last.fm/api/account/create."
      },
      username: { type: "string", label: "Last.fm username" },
      showAlbum: { type: "boolean", label: "Show album name" },
      mode: {
        type: "select",
        label: "Activity type",
        options: [
          { label: "Listening", value: "listening" },
          { label: "Playing", value: "playing" }
        ]
      }
    },
    async start() {
      const dispatcher2 = findDispatcher();
      if (!dispatcher2?.dispatch) {
        log15.warn("dispatcher not found \u2014 plugin inactive");
        return;
      }
      let timer2;
      let lastTrack = "";
      const tick = async () => {
        const apiKey = await Api.Settings.get(
          "plugins.settings.MusicRichPresence.apiKey"
        );
        const username = await Api.Settings.get(
          "plugins.settings.MusicRichPresence.username"
        );
        if (!apiKey || !username) {
          if (lastTrack) {
            lastTrack = "";
            dispatcher2.dispatch({
              type: "LOCAL_ACTIVITY_UPDATE",
              activity: null,
              socketId: "MusicRichPresence"
            });
          }
          return;
        }
        try {
          const url = `https://ws.audioscrobbler.com/2.0/?method=user.getrecenttracks&user=${encodeURIComponent(username)}&api_key=${encodeURIComponent(apiKey)}&format=json&limit=1`;
          const res = await fetch(url);
          const data = await res.json();
          const track = data.recenttracks?.track?.[0];
          const name = String(track?.name ?? "");
          const artist = isObj(track?.artist) ? String(track.artist["#text"] ?? "") : "";
          if (!name || name === lastTrack) return;
          lastTrack = name;
          const showAlbum = await Api.Settings.get(
            "plugins.settings.MusicRichPresence.showAlbum",
            true
          ) !== false;
          const album = showAlbum ? String(track?.album ?? "") : "";
          const mode = await Api.Settings.get(
            "plugins.settings.MusicRichPresence.mode",
            "listening"
          ) === "playing" ? 0 : 2;
          dispatcher2.dispatch({
            type: "LOCAL_ACTIVITY_UPDATE",
            activity: {
              name: "Last.fm",
              type: mode,
              details: name,
              state: [artist, album].filter(Boolean).join(" \u2014 ") || void 0,
              assets: {
                large_image: "https://www.last.fm/static/images/lastfm_avatar_walk.png",
                large_text: "Last.fm"
              },
              application_id: "0"
            },
            socketId: "MusicRichPresence"
          });
        } catch (e) {
          log15.debug("last.fm fetch failed", e);
        }
      };
      void tick();
      timer2 = setInterval(() => void tick(), 3e4);
      this.__timer = timer2;
    },
    stop() {
      const t = this.__timer;
      if (t) clearInterval(t);
      const dispatcher2 = findDispatcher();
      dispatcher2?.dispatch({
        type: "LOCAL_ACTIVITY_UPDATE",
        activity: null,
        socketId: "MusicRichPresence"
      });
    }
  };
  var SpotifyControls = {
    name: "SpotifyControls",
    description: "Embed a Spotify player (artist/album/track/playlist URI) in the sidebar.",
    authors: ["Monocord"],
    version: "0.1.0",
    settings: { spotifyUri: "" },
    settingsSchema: {
      spotifyUri: {
        type: "string",
        label: "Spotify URI or URL",
        description: "e.g. spotify:album:4LH4d3cOWNNsVw41Gqt2kv or an open.spotify.com link.",
        placeholder: "spotify:track:\u2026"
      }
    },
    async start() {
      const raw = await Api.Settings.get(
        "plugins.settings.SpotifyControls.spotifyUri"
      );
      const uri = (raw ?? "").trim();
      if (!uri) return;
      const m = /spotify:(\w+):([A-Za-z0-9]+)/.exec(uri) ?? /open\.spotify\.com\/(\w+)\/([A-Za-z0-9]+)/.exec(uri);
      if (!m) {
        log15.warn("invalid spotify uri \u2014 plugin inactive");
        return;
      }
      const [, kind, id] = m;
      if (!kind || !id) return;
      const src = `https://open.spotify.com/embed/${kind}/${id}?utm_source=generator`;
      const inject = () => {
        const panel = document.querySelector(
          '[class*="panels"], [class*="accountPanel"], nav [class*="wrapper"]'
        );
        if (!panel || panel.querySelector("[data-mc-spotify]")) return;
        const wrap2 = el("div", {
          padding: "8px",
          borderTop: "1px solid var(--background-modifier-accent,#3f4147)"
        });
        wrap2.dataset.mcSpotify = "1";
        const iframe = document.createElement("iframe");
        iframe.src = src;
        iframe.style.cssText = "border-radius:8px;border:0;width:100%;height:152px;display:block;";
        iframe.allow = "autoplay; clipboard-write; encrypted-media; fullscreen; picture-in-picture";
        iframe.loading = "lazy";
        wrap2.append(iframe);
        panel.append(wrap2);
      };
      inject();
      const stopObserve = observeAdded('[class*="accountPanel"], [class*="panels"]', inject);
      this.__cleanup = () => {
        stopObserve();
        document.querySelectorAll("[data-mc-spotify]").forEach((n) => n.remove());
      };
      Api.Events.on("settings.changed", (payload) => {
        const path = payload?.path;
        if (path === "plugins.settings.SpotifyControls.spotifyUri") {
          location.reload();
        }
      });
    },
    stop() {
      this.__cleanup?.();
    }
  };
  var WhoReacted = {
    name: "WhoReacted",
    description: "Show who reacted to a message in the reaction tooltip.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const dispatcher2 = findDispatcher();
      const getUser = storeMethod2(
        "UserStore",
        "getUser"
      );
      if (!dispatcher2 || !getUser) {
        log15.warn("dispatcher/UserStore missing \u2014 plugin inactive");
        return;
      }
      const reactions = /* @__PURE__ */ new Map();
      const key = (emoji) => `${String(emoji.name ?? "")}:${String(emoji.id ?? "")}`;
      const sub = dispatcher2.subscribe((action) => {
        const type = action.type;
        if (type !== "MESSAGE_REACTION_ADD" && type !== "MESSAGE_REACTION_REMOVE") return;
        const msgObj = isObj(action.message) ? action.message : null;
        const channelId = String(action.channelId ?? msgObj?.channel_id ?? "");
        const messageId = String(action.messageId ?? msgObj?.id ?? "");
        const userId = String(action.userId ?? "");
        const emoji = isObj(action.emoji) ? action.emoji : null;
        if (!channelId || !messageId || !userId || !emoji) return;
        const mapKey = `${channelId}:${messageId}`;
        let byEmoji = reactions.get(mapKey);
        if (!byEmoji) {
          byEmoji = { users: /* @__PURE__ */ new Map(), emojiKey: "" };
          reactions.set(mapKey, byEmoji);
        }
        const ek = key(emoji);
        const users = byEmoji[ek] ?? (byEmoji[ek] = /* @__PURE__ */ new Map());
        if (type === "MESSAGE_REACTION_ADD") {
          const u = getUser(userId);
          const name = String(
            u?.global_name ?? u?.username ?? userId
          );
          users.set(userId, name);
        } else {
          users.delete(userId);
        }
      });
      const nameFor = (userId) => {
        const u = getUser(userId);
        return String(
          u?.global_name ?? u?.username ?? userId
        );
      };
      observeAdded('[class*="reaction"]', (node) => {
        const chip = node;
        if (chip.dataset.mcWhoReacted) return;
        const msgRoot = chip.closest('[id^="chat-messages-"]');
        if (!msgRoot) return;
        const ids = /chat-messages-(\d+)-(\d+)/.exec(msgRoot.id);
        if (!ids?.[1] || !ids[2]) return;
        const entry = reactions.get(`${ids[1]}:${ids[2]}`);
        if (!entry) return;
        const emojiEl = chip.querySelector('[class*="emoji"], img[alt]');
        const alt = emojiEl?.getAttribute("alt") ?? "";
        let names = [];
        for (const [ek, users] of Object.entries(
          entry
        )) {
          const emojiName = ek.split(":")[0];
          if (emojiName && alt && (alt.includes(emojiName) || emojiName.includes(alt))) {
            names = [...users.values()];
            break;
          }
        }
        if (!names.length) return;
        chip.dataset.mcWhoReacted = "1";
        chip.title = `Reacted by: ${names.slice(0, 20).join(", ")}`;
      });
      this.__sub = sub;
    },
    stop() {
      const sub = this.__sub;
      sub?.unsubscribe?.();
    }
  };
  var FullSearchContext = {
    name: "FullSearchContext",
    description: "Right-click search results for Copy Text / Copy Link / Reply.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const off = on(
        document,
        "contextmenu",
        '[class*="searchResult"], [class*="searchResultMessage"], li[class*="message"]',
        (el2, ev) => {
          const msgRoot = el2.closest('[id^="chat-messages-"]');
          if (!msgRoot) return;
          const text = msgRoot.textContent ?? "";
          const ids = /chat-messages-(\d+)-(\d+)/.exec(msgRoot.id);
          const items = [
            {
              label: "Copy Text",
              action: () => void navigator.clipboard.writeText(text)
            }
          ];
          if (ids?.[1] && ids[2]) {
            items.push({
              label: "Copy Message Link",
              action: () => void navigator.clipboard.writeText(
                `https://discord.com/channels/${ids[1]}/${ids[2]}`
              )
            });
          }
          const mev = ev;
          ev.preventDefault();
          contextMenu(mev.clientX, mev.clientY, items);
        }
      );
      this.__off = off;
    },
    stop() {
      this.__off?.();
    }
  };
  var GameActivityToggle = {
    name: "GameActivityToggle",
    description: "Toggle game activity sharing from a button near voice controls.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const settingsMod = Api.Webpack.findAllByProps("showCurrentGame");
      let setter = null;
      for (const exp of settingsMod) {
        const o = isObj(exp) ? exp : null;
        const d = o && isObj(o.default) ? o.default : null;
        for (const target of [o, d]) {
          if (!target) continue;
          for (const [k2, v] of Object.entries(target)) {
            if (typeof v === "function" && /update|set/i.test(k2)) {
              setter = (val) => {
                try {
                  v.call(target, {
                    status: { showCurrentGame: val }
                  });
                } catch {
                  try {
                    v.call(target, val);
                  } catch {
                  }
                }
              };
            }
          }
        }
      }
      if (!setter) {
        log15.warn("user settings module not found \u2014 plugin inactive");
        return;
      }
      let enabled = true;
      const inject = () => {
        const anchor = document.querySelector(
          '[class*="panels"] [class*="buttons"], [class*="voiceControls"]'
        );
        if (!anchor || anchor.querySelector("[data-mc-gameact]")) return;
        const btn = el(
          "button",
          {
            marginLeft: "4px",
            padding: "2px 8px",
            fontSize: "12px",
            borderRadius: "4px",
            border: "1px solid var(--background-modifier-accent,#3f4147)",
            background: "var(--background-secondary,#2b2d31)",
            color: "var(--text-normal,#dbdee1)",
            cursor: "pointer"
          },
          "Game Activity"
        );
        btn.dataset.mcGameact = "1";
        btn.title = "Toggle game activity sharing";
        btn.addEventListener("click", () => {
          enabled = !enabled;
          setter?.(enabled);
          btn.style.opacity = enabled ? "1" : "0.5";
        });
        anchor.append(btn);
      };
      inject();
      const stopObserve = observeAdded('[class*="voiceControls"], [class*="panels"]', inject);
      this.__cleanup = () => {
        stopObserve();
        document.querySelectorAll("[data-mc-gameact]").forEach((n) => n.remove());
      };
    },
    stop() {
      this.__cleanup?.();
    }
  };
  var NoReplyMention = {
    name: "NoReplyMention",
    description: "Send replies without pinging the original author.",
    authors: ["Monocord"],
    version: "0.1.0",
    settings: { enabled: true },
    settingsSchema: {
      enabled: { type: "boolean", label: "Don't ping on reply" }
    },
    start() {
      const mod = findSendModule();
      if (!mod) {
        log15.warn("send module not found \u2014 plugin inactive");
        return;
      }
      Api.Patcher.before(this.name, mod, "sendMessage", (...args) => {
        const messageData = args[1];
        const options = args[2];
        if (!isObj(messageData) || !isObj(messageData.messageReference)) return;
        const ref = messageData.messageReference;
        if (!isObj(ref) || !ref.message_id) return;
        for (const target of [messageData, options]) {
          if (!isObj(target)) continue;
          for (const key of ["shouldMention", "mentioned", "mention"]) {
            if (key in target) target[key] = false;
          }
        }
      });
    }
  };
  var OverrideForumDefaults = {
    name: "OverrideForumDefaults",
    description: "Override default forum layout (List/Gallery) and sort order.",
    authors: ["Monocord"],
    version: "0.1.0",
    settings: { layout: 1, sortOrder: 0 },
    settingsSchema: {
      layout: {
        type: "select",
        label: "Default layout",
        options: [
          { label: "List", value: "1" },
          { label: "Gallery", value: "2" }
        ]
      },
      sortOrder: {
        type: "select",
        label: "Default sort order",
        options: [
          { label: "Recently Active", value: "0" },
          { label: "Date Posted", value: "1" }
        ]
      }
    },
    start() {
      let layout = 1;
      let sortOrder = 0;
      const KEY2 = "plugins.settings.OverrideForumDefaults.";
      const load = async () => {
        layout = await Api.Settings.get(`${KEY2}layout`, 1) ?? 1;
        sortOrder = await Api.Settings.get(`${KEY2}sortOrder`, 0) ?? 0;
      };
      void load();
      Api.Events.on("settings.changed", (payload) => {
        const path = payload?.path;
        if (typeof path === "string" && path.startsWith(KEY2)) void load();
      });
      let patched = 0;
      for (const exp of Api.Webpack.findAllByProps("getDefaultLayout")) {
        const targets = [exp];
        if (isObj(exp) && isObj(exp.default)) targets.push(exp.default);
        for (const t of targets) {
          if (!isObj(t)) continue;
          if (typeof t.getDefaultLayout === "function") {
            Api.Patcher.instead(this.name, t, "getDefaultLayout", () => layout);
            patched++;
          }
          if (typeof t.getDefaultSortOrder === "function") {
            Api.Patcher.instead(this.name, t, "getDefaultSortOrder", () => sortOrder);
            patched++;
          }
        }
      }
      if (!patched) log15.warn("forum defaults module not found \u2014 plugin inactive");
    }
  };
  var MoreQuickReactions = {
    name: "MoreQuickReactions",
    description: "Increase quick reactions shown on message hover (best-effort).",
    authors: ["Monocord"],
    version: "0.1.0",
    settings: { count: 5 },
    settingsSchema: {
      count: { type: "number", label: "Reaction count (0-42)" }
    },
    start() {
      let count = 5;
      const KEY2 = "plugins.settings.MoreQuickReactions.count";
      void Api.Settings.get(KEY2, 5).then((v) => count = v ?? 5);
      Api.Events.on("settings.changed", (payload) => {
        const path = payload?.path;
        if (path === KEY2) void Api.Settings.get(KEY2, 5).then((v) => count = v ?? 5);
      });
      const probes = [
        Api.Webpack.findByProps("quickReactions"),
        Api.Webpack.findByProps("getQuickReactions")
      ];
      let ok = false;
      for (const p of probes) {
        if (isObj(p)) {
          for (const [k2, v] of Object.entries(p)) {
            if (typeof v === "function" && /reaction/i.test(k2)) {
              Api.Patcher.after(this.name, p, k2, (result) => {
                if (Array.isArray(result)) {
                  return result.slice(0, Math.max(0, Math.min(42, count)));
                }
                return result;
              });
              ok = true;
            }
          }
        }
      }
      if (!ok) log15.warn("quick reactions module not found \u2014 plugin inactive");
    }
  };
  var NoServerEmojis = {
    name: "NoServerEmojis",
    description: "Hide server emojis from the emoji autocomplete.",
    authors: ["Monocord"],
    version: "0.1.0",
    settings: { mode: "onlyUnicode" },
    settingsSchema: {
      mode: {
        type: "select",
        label: "Shown emojis",
        options: [
          { label: "Only unicode emojis", value: "onlyUnicode" },
          { label: "Unicode + current server", value: "currentServer" },
          { label: "All (Discord default)", value: "all" }
        ]
      }
    },
    start() {
      let mode = "onlyUnicode";
      const KEY2 = "plugins.settings.NoServerEmojis.mode";
      void Api.Settings.get(KEY2, "onlyUnicode").then((v) => mode = v ?? "onlyUnicode");
      Api.Events.on("settings.changed", (payload) => {
        const path = payload?.path;
        if (path === KEY2) void Api.Settings.get(KEY2, "onlyUnicode").then((v) => mode = v ?? "onlyUnicode");
      });
      const candidates = Api.Webpack.findAllByProps("searchWithoutFetchingLatest");
      let patched = 0;
      for (const exp of candidates) {
        const targets = [exp];
        if (isObj(exp) && isObj(exp.default)) targets.push(exp.default);
        for (const t of targets) {
          if (!isObj(t)) continue;
          const fn = t.searchWithoutFetchingLatest;
          if (typeof fn !== "function") continue;
          Api.Patcher.after(this.name, t, "searchWithoutFetchingLatest", (result) => {
            if (mode === "all" || !Array.isArray(result)) return result;
            return result.filter((e) => {
              if (!isObj(e)) return true;
              if (e.type !== 1) return true;
              return mode === "currentServer";
            });
          });
          patched++;
        }
      }
      if (!patched) log15.warn("emoji search module not found \u2014 plugin inactive");
    }
  };
  var FavoriteEmojiFirst = {
    name: "FavoriteEmojiFirst",
    description: "Show favorite emoji first in the emoji picker (best-effort).",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      const store = findStoreByName("EmojiStore");
      if (!store) {
        log15.warn("EmojiStore not found \u2014 plugin inactive");
        return;
      }
      let patched = 0;
      const getFavs = store.getFavorites;
      for (const key of Object.keys(store)) {
        const v = store[key];
        if (typeof v !== "function" || !/emoji|favorite/i.test(key)) continue;
        Api.Patcher.after(this.name, store, key, (result) => {
          if (!Array.isArray(result)) return result;
          const favs = typeof getFavs === "function" ? getFavs.call(store) : [];
          const favIds = new Set(
            favs.map((f) => `${f.name}:${f.id ?? ""}`)
          );
          if (!favIds.size) return result;
          const rank = (e) => {
            if (!isObj(e)) return 1;
            return favIds.has(`${String(e.name ?? "")}:${String(e.id ?? "")}`) ? 0 : 1;
          };
          return [...result].sort((a, b) => rank(a) - rank(b));
        });
        patched++;
      }
      if (!patched) log15.warn("no emoji methods found on EmojiStore \u2014 plugin inactive");
    }
  };
  var WebScreenShareFixes = {
    name: "WebScreenShareFixes",
    description: "Attempt to raise screenshare bitrate/framerate caps (best-effort).",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      let patched = 0;
      for (const exp of Api.Webpack.findAllByProps("maxBitrate", "frameRate")) {
        const targets = [exp];
        if (isObj(exp) && isObj(exp.default)) targets.push(exp.default);
        for (const t of targets) {
          if (!isObj(t)) continue;
          for (const key of ["maxBitrate", "frameRate", "maxFrameRate"]) {
            const v = t[key];
            if (typeof v === "number" && v > 0 && v <= 2500) {
              try {
                t[key] = key === "maxBitrate" ? 8e3 : 60;
                patched++;
              } catch {
              }
            }
          }
        }
      }
      if (!patched) {
        log15.warn("screenshare settings module not found \u2014 plugin inactive");
      } else {
        log15.info(`raised ${patched} screenshare caps`);
      }
    }
  };
  var PLUGINS2 = [
    ConsoleShortcuts,
    MusicRichPresence,
    SpotifyControls,
    WhoReacted,
    FullSearchContext,
    GameActivityToggle,
    NoReplyMention,
    OverrideForumDefaults,
    MoreQuickReactions,
    NoServerEmojis,
    FavoriteEmojiFirst,
    WebScreenShareFixes
  ];
  function registerVencordLast() {
    for (const p of PLUGINS2) {
      try {
        Api.Plugins.register(p);
      } catch (e) {
        log15.warn(`register ${p.name} failed`, e);
      }
    }
  }

  // src/ui/react.ts
  var log16 = Logger.get("ui");
  var react = null;
  async function ensureReact() {
    if (react) return react;
    react = await Api.React.get();
    return react;
  }
  function getReact2() {
    if (!react) throw new Error("React not initialized \u2014 call ensureReact()");
    return react;
  }
  function h(type, props, ...children) {
    return getReact2().createElement(type, props, ...children);
  }
  function logReactInfo() {
    const r = getReact2();
    log16.info("react resolved", {
      keys: Object.keys(r).slice(0, 20),
      hasMemo: typeof r.useMemo,
      hasRef: typeof r.useRef
    });
  }

  // src/ui/settings.ts
  var log17 = Logger.get("settings-ui");
  var PanelBase = class {
    props;
    state;
    refs = {};
    updater = null;
    constructor(props) {
      this.props = props ?? {};
      this.state = {};
    }
    setState(update, callback) {
      const partial = typeof update === "function" ? update(this.state, this.props) : update;
      this.updater?.enqueueSetState?.(this, partial, callback, "setState");
    }
    forceUpdate(callback) {
      this.updater?.enqueueForceUpdate?.(this, callback, "forceUpdate");
    }
    render() {
      return null;
    }
  };
  PanelBase.prototype.isReactComponent = {};
  var S = {
    page: {
      padding: "16px 20px",
      color: "var(--text-normal, #dbdee1)",
      fontSize: 14,
      lineHeight: "20px"
    },
    h2: { fontSize: 20, fontWeight: 600, margin: "0 0 12px" },
    muted: { color: "var(--text-muted, #949ba4)", fontSize: 12 },
    card: {
      background: "var(--background-secondary, #2b2d31)",
      borderRadius: 8,
      padding: "12px 16px",
      marginBottom: 8
    },
    row: {
      display: "flex",
      alignItems: "center",
      justifyContent: "space-between",
      gap: 12
    },
    btn: {
      background: "var(--brand-500, #5865f2)",
      color: "#fff",
      border: "none",
      borderRadius: 4,
      padding: "6px 14px",
      fontSize: 13,
      fontWeight: 500,
      cursor: "pointer"
    },
    btnOff: {
      background: "var(--background-modifier-accent, #3f4147)",
      color: "var(--text-normal, #dbdee1)",
      border: "1px solid var(--background-modifier-accent, #3f4147)",
      borderRadius: 4,
      padding: "5px 13px",
      fontSize: 13,
      cursor: "pointer"
    },
    input: {
      width: "100%",
      background: "var(--input-background, #1e1f22)",
      color: "var(--text-normal, #dbdee1)",
      border: "1px solid var(--background-modifier-accent, #3f4147)",
      borderRadius: 4,
      padding: "8px 10px",
      fontSize: 13,
      fontFamily: "var(--font-code, monospace)",
      boxSizing: "border-box"
    },
    badge: {
      display: "inline-block",
      padding: "1px 8px",
      borderRadius: 10,
      fontSize: 11,
      fontWeight: 600,
      marginLeft: 8,
      verticalAlign: "middle"
    }
  };
  function statusDot(enabled, running) {
    const cls = !enabled ? "off" : running ? "on" : "warn";
    const tip = !enabled ? "Disabled" : running ? "Enabled \xB7 running" : "Enabled \xB7 not running";
    return h("span", { className: `mc-status ${cls}`, title: tip });
  }
  var OverviewPanel = class extends PanelBase {
    state = { info: null };
    componentDidMount() {
      ensurePluginsCss();
      const bridge5 = window.monocord;
      if (bridge5) {
        bridge5.invoke("version").then((v) => this.setState({ info: v })).catch(() => {
        });
      }
    }
    render() {
      const info = this.state.info;
      const pluginCount = Api.Plugins.names().length;
      const enabledCount = Api.Plugins.names().filter(
        (n) => Api.Plugins.isEnabled(n)
      ).length;
      const failedCount = Api.Plugins.getFailures().length;
      const stats = [
        ["Framework", `v${Api.version}`],
        ["Discord", String(info?.discord ?? "unknown")],
        ["Host", "Electron"],
        ["Plugins", `${enabledCount} enabled / ${pluginCount}`],
        ...failedCount > 0 ? [["Failed plugins", String(failedCount)]] : [],
        [
          "Theme",
          Themes.enabled.length === 0 ? "None" : Themes.enabled.length === 1 ? Themes.enabled[0] ?? "" : `${Themes.enabled.length} enabled`
        ]
      ];
      return h(
        "div",
        { style: S.page },
        h("h2", { style: S.h2 }, "Monocord"),
        h(
          "div",
          { className: "mc-grid" },
          stats.map(
            ([label, value]) => h(
              "div",
              { key: label, className: "mc-card" },
              h("div", { className: "mc-author" }, label),
              h("div", { className: "mc-name", style: { fontSize: 18 } }, value)
            )
          )
        ),
        h(
          "div",
          { className: "mc-card", style: { marginTop: 12 } },
          h(
            "div",
            { className: "mc-desc", style: { minHeight: 0, opacity: 1 } },
            "Monocord is a Vencord-inspired modification framework. Manage plugins from the Plugins tab and themes from the Themes tab."
          )
        )
      );
    }
  };
  var PLUGIN_EVENTS = [
    "plugin.enabled",
    "plugin.disabled",
    "plugin.error",
    "plugin.started",
    "plugin.stopped",
    "plugin.registered",
    "plugin.unregistered"
  ];
  var PLUGINS_CSS = `
.mc-grid{display:grid;gap:12px;grid-template-columns:repeat(auto-fill,minmax(250px,1fr));}
.mc-card{background:var(--card-background-default,var(--background-secondary,#2b2d31));border:1px solid var(--border-subtle,rgba(255,255,255,.06));border-radius:8px;padding:12px 14px;transition:transform .1s ease,box-shadow .1s ease,border-color .1s ease;box-sizing:border-box;}
.mc-card:hover{transform:translateY(-1px);box-shadow:0 8px 16px rgba(0,0,0,.28);border-color:var(--border-subtle,#3f4147);}
.mc-card-off{opacity:.55;}
.mc-card-off:hover{opacity:.8;}
.mc-name{font-weight:700;font-size:15px;display:flex;align-items:center;gap:8px;min-width:0;}
.mc-ver{font-size:11px;color:var(--text-muted,#949ba4);font-weight:400;flex-shrink:0;}
.mc-author{font-size:12px;color:var(--text-muted,#949ba4);margin-top:2px;}
.mc-desc{margin-top:8px;font-size:12px;line-height:16px;color:var(--text-normal,#dbdee1);opacity:.75;display:-webkit-box;-webkit-line-clamp:2;-webkit-box-orient:vertical;overflow:hidden;min-height:32px;}
.mc-head{display:flex;align-items:center;justify-content:space-between;gap:8px;}
.mc-head-l{display:flex;align-items:center;gap:6px;min-width:0;flex:1;}
.mc-icon-btn{background:transparent;border:none;color:var(--icon-muted,#80848e);cursor:pointer;font-size:15px;padding:4px;border-radius:4px;line-height:1;flex-shrink:0;}
.mc-icon-btn:hover{color:var(--interactive-icon-hover,#dbdee1);background:var(--background-modifier-hover,rgba(78,80,88,.35));}
.mc-back{background:transparent;border:none;color:var(--brand-500,#5865f2);cursor:pointer;font-size:13px;padding:4px 0;margin-bottom:8px;}
.mc-back:hover{text-decoration:underline;}
.mc-detail{max-width:680px;}
.mc-toggle{position:relative;width:38px;height:22px;border-radius:11px;border:none;cursor:pointer;padding:0;flex-shrink:0;background:var(--background-modifier-accent,#4e5058);transition:background .15s;}
.mc-toggle.on{background:var(--status-positive,#23a55a);}
.mc-toggle>span{position:absolute;top:2px;left:2px;width:18px;height:18px;border-radius:50%;background:#fff;transition:left .15s;box-shadow:0 1px 3px rgba(0,0,0,.4);}
.mc-toggle.on>span{left:18px;}
.mc-filters{display:flex;gap:8px;margin:10px 0 14px;align-items:center;}
.mc-chip{background:var(--background-secondary,#2b2d31);border:1px solid var(--background-modifier-accent,#3f4147);color:var(--text-normal,#dbdee1);border-radius:16px;padding:5px 14px;font-size:13px;cursor:pointer;transition:background .12s,border-color .12s,color .12s;}
.mc-chip:hover{border-color:var(--brand-500,#5865f2);}
.mc-chip.on{background:var(--brand-500,#5865f2);border-color:var(--brand-500,#5865f2);color:#fff;}
.mc-chip .mc-count{opacity:.65;font-size:11px;margin-left:4px;}
.mc-card-link{cursor:pointer;}
.mc-card-link:focus-visible{outline:2px solid var(--brand-500,#5865f2);outline-offset:2px;}
.mc-status{width:8px;height:8px;border-radius:50%;flex-shrink:0;display:inline-block;}
.mc-status.on{background:var(--status-positive,#23a55a);box-shadow:0 0 6px rgba(35,165,90,.55);}
.mc-status.warn{background:var(--status-warning,#f0b232);}
.mc-status.off{background:var(--background-modifier-accent,#4e5058);}
.mc-warn{background:var(--background-warning-solid,#59421e);border:1px solid var(--status-warning,#f0b232);border-radius:8px;padding:12px 14px;margin-bottom:12px;font-size:13px;line-height:18px;color:var(--text-normal,#dbdee1);}
.mc-warn code{background:rgba(0,0,0,.25);border-radius:3px;padding:0 4px;font-family:var(--font-code,monospace);font-size:12px;}
.mc-keys{display:flex;gap:6px;margin-top:8px;flex-wrap:wrap;}
.mc-key{font-size:11px;color:var(--text-muted,#949ba4);border:1px solid var(--border-subtle,rgba(255,255,255,.08));border-radius:4px;padding:2px 6px;font-family:var(--font-code,monospace);}
`;
  var pluginsCssInjected = false;
  function ensurePluginsCss() {
    if (pluginsCssInjected) return;
    pluginsCssInjected = true;
    Api.Themes.injectCss("monocord-plugins-ui", PLUGINS_CSS);
  }
  function pluginRows() {
    return Api.Plugins.names().filter((n) => n !== "MonocordSettings").map((name) => ({
      name,
      plugin: Api.Plugins.get(name),
      enabled: Api.Plugins.isEnabled(name),
      running: Api.Plugins.isRunning(name)
    }));
  }
  var PluginsPanel = class extends PanelBase {
    state = {
      rev: 0,
      drafts: {},
      errors: {},
      values: {},
      query: "",
      filter: "all",
      detail: null
    };
    offs = [];
    componentDidMount() {
      ensurePluginsCss();
      this.offs = PLUGIN_EVENTS.map(
        (e) => Events.on(e, () => this.setState({ rev: this.state.rev + 1 }))
      );
    }
    componentWillUnmount() {
      this.offs.forEach((off) => off());
      this.offs = [];
      for (const [tkey, timer2] of this.persistTimers) {
        clearTimeout(timer2);
        const dot = tkey.indexOf(".");
        const name = tkey.slice(0, dot);
        const key = tkey.slice(dot + 1);
        const value = this.state.values[name]?.[key];
        if (value !== void 0) {
          void Settings.set(`plugins.settings.${name}.${key}`, value).catch(
            () => {
            }
          );
        }
      }
      this.persistTimers.clear();
    }
    settingKeys(plugin) {
      return plugin.settings ? Object.keys(plugin.settings) : [];
    }
    async loadDraft(name, plugin) {
      const values = {};
      for (const key of this.settingKeys(plugin)) {
        const v = await Settings.get(
          `plugins.settings.${name}.${key}`,
          plugin.settings?.[key]
        );
        values[key] = v ?? plugin.settings?.[key];
      }
      this.setState({
        drafts: {
          ...this.state.drafts,
          [name]: JSON.stringify(values, null, 2)
        },
        errors: { ...this.state.errors, [name]: "" },
        values: { ...this.state.values, [name]: values }
      });
    }
    // Debounce settings.json writes — custom views (e.g. CustomRPC) call
    // set() per keystroke, and each write is a full-document IPC save.
    persistTimers = /* @__PURE__ */ new Map();
    setSetting(name, key, value, plugin) {
      const cur = this.state.values[name] ?? {};
      this.setState({
        values: { ...this.state.values, [name]: { ...cur, [key]: value } }
      });
      const tkey = `${name}.${key}`;
      const prev = this.persistTimers.get(tkey);
      if (prev) clearTimeout(prev);
      this.persistTimers.set(
        tkey,
        setTimeout(() => {
          this.persistTimers.delete(tkey);
          void Settings.set(`plugins.settings.${name}.${key}`, value).catch(
            (e) => log17.error(`settings set ${name}.${key} failed`, e)
          );
        }, 400)
      );
    }
    renderField(name, plugin, key, field) {
      const value = this.state.values[name]?.[key] ?? plugin.settings?.[key];
      const set = (v) => this.setSetting(name, key, v, plugin);
      const label = field.label ?? key;
      const common = { disabled: !Api.Plugins.isEnabled(name) };
      let control;
      switch (field.type) {
        case "boolean":
          control = h(
            "button",
            {
              role: "switch",
              "aria-checked": value === true,
              disabled: common.disabled,
              onClick: () => set(value !== true),
              style: {
                position: "relative",
                width: 40,
                height: 22,
                borderRadius: 11,
                border: "none",
                cursor: common.disabled ? "default" : "pointer",
                padding: 0,
                flexShrink: 0,
                background: value === true ? "var(--status-positive, #23a55a)" : "var(--background-modifier-accent, #4e5058)",
                transition: "background .15s"
              }
            },
            h("span", {
              style: {
                position: "absolute",
                top: 2,
                left: value === true ? 20 : 2,
                width: 18,
                height: 18,
                borderRadius: "50%",
                background: "#fff",
                transition: "left .15s",
                boxShadow: "0 1px 3px rgba(0,0,0,.4)"
              }
            })
          );
          break;
        case "select":
          control = h(
            "select",
            {
              ...common,
              value: String(value ?? ""),
              onChange: (e) => set(e.target.value),
              style: { ...S.input, width: "auto", minWidth: 140 }
            },
            (field.options ?? []).map(
              (o) => h("option", { key: o.value, value: o.value }, o.label)
            )
          );
          break;
        case "number":
          control = h("input", {
            ...common,
            type: "number",
            value: value == null ? "" : String(value),
            placeholder: field.placeholder,
            onChange: (e) => set(e.target.value === "" ? null : Number(e.target.value)),
            style: { ...S.input, width: 90 }
          });
          break;
        case "textarea":
          control = h("textarea", {
            ...common,
            value: String(value ?? ""),
            placeholder: field.placeholder,
            onChange: (e) => set(e.target.value),
            style: { ...S.input, minHeight: 64, resize: "vertical" }
          });
          break;
        default:
          control = h("input", {
            ...common,
            type: "text",
            value: String(value ?? ""),
            placeholder: field.placeholder,
            onChange: (e) => set(e.target.value),
            style: S.input
          });
      }
      return h(
        "div",
        { key, style: { marginTop: 10 } },
        h(
          "div",
          { style: { ...S.row, alignItems: "flex-start" } },
          h(
            "label",
            { style: { fontSize: 13, fontWeight: 500, flex: "0 0 160px" } },
            label
          ),
          h("div", { style: { flex: 1, minWidth: 0 } }, control)
        ),
        field.description ? h("div", { style: { ...S.muted, marginTop: 3, marginLeft: 172 } }, field.description) : null
      );
    }
    saveSettings(name, plugin) {
      const raw = this.state.drafts[name] ?? "";
      let parsed;
      try {
        parsed = JSON.parse(raw);
      } catch (e) {
        this.setState({
          errors: {
            ...this.state.errors,
            [name]: `Invalid JSON: ${String(e)}`
          }
        });
        return;
      }
      void (async () => {
        for (const key of this.settingKeys(plugin)) {
          if (key in parsed) {
            await Settings.set(
              `plugins.settings.${name}.${key}`,
              parsed[key]
            );
          }
        }
        this.setState({
          errors: { ...this.state.errors, [name]: "" },
          drafts: { ...this.state.drafts, [name]: "" }
        });
        void this.loadDraft(name, plugin);
      })();
    }
    openDetail(name, plugin) {
      this.setState({ detail: name });
      if (!this.state.values[name] && !this.state.drafts[name]) {
        void this.loadDraft(name, plugin);
      }
    }
    togglePlugin(name, enabled) {
      const p = enabled ? Api.Plugins.disable(name) : Api.Plugins.enable(name);
      void p.catch((e) => log17.error(`toggle ${name} failed`, e));
    }
    renderToggle(name, enabled) {
      return h(
        "button",
        {
          className: `mc-toggle${enabled ? " on" : ""}`,
          role: "switch",
          "aria-checked": enabled,
          title: enabled ? "Disable" : "Enable",
          onClick: (e) => {
            e.stopPropagation?.();
            this.togglePlugin(name, enabled);
          }
        },
        h("span", null)
      );
    }
    renderSettingsBody(name, plugin) {
      const { drafts, errors } = this.state;
      if (plugin.settingsView) {
        const ctx = {
          h,
          values: this.state.values[name] ?? {},
          set: (key, value) => this.setSetting(name, key, value, plugin),
          enabled: Api.Plugins.isEnabled(name)
        };
        return h(
          "div",
          { style: { marginTop: 14 } },
          plugin.settingsView(ctx),
          // Escape hatch: raw JSON editor for power users.
          h(
            "details",
            { style: { marginTop: 14 } },
            h("summary", { style: S.muted }, "Advanced: raw JSON"),
            h("textarea", {
              style: {
                ...S.input,
                minHeight: 120,
                resize: "vertical",
                marginTop: 8
              },
              value: drafts[name] ?? "",
              placeholder: "{}",
              onChange: (e) => this.setState({
                drafts: {
                  ...this.state.drafts,
                  [name]: e.target.value
                }
              })
            }),
            h(
              "div",
              {
                style: {
                  ...S.row,
                  marginTop: 8,
                  justifyContent: "flex-start"
                }
              },
              h(
                "button",
                {
                  style: S.btnOff,
                  onClick: () => this.saveSettings(name, plugin)
                },
                "Save JSON"
              )
            )
          )
        );
      }
      if (plugin.settingsSchema) {
        return h(
          "div",
          { style: { marginTop: 14, paddingLeft: 4 } },
          Object.entries(plugin.settingsSchema).map(
            ([key, field]) => this.renderField(name, plugin, key, field)
          ),
          errors[name] ? h(
            "div",
            {
              style: {
                ...S.muted,
                color: "var(--status-danger,#f23f43)",
                marginTop: 6
              }
            },
            errors[name]
          ) : null
        );
      }
      return h(
        "div",
        { style: { marginTop: 14 } },
        h("textarea", {
          style: { ...S.input, minHeight: 140, resize: "vertical" },
          value: drafts[name] ?? "",
          placeholder: "{}",
          onChange: (e) => this.setState({
            drafts: { ...this.state.drafts, [name]: e.target.value }
          })
        }),
        h(
          "div",
          {
            style: {
              ...S.row,
              marginTop: 8,
              justifyContent: "flex-start"
            }
          },
          h(
            "button",
            {
              style: S.btn,
              onClick: () => this.saveSettings(name, plugin)
            },
            "Save settings"
          ),
          errors[name] ? h(
            "span",
            {
              style: {
                ...S.muted,
                color: "var(--status-danger,#f23f43)"
              }
            },
            errors[name]
          ) : null
        )
      );
    }
    renderDetail(name, plugin, enabled) {
      const hasSettings = this.settingKeys(plugin).length > 0;
      return h(
        "div",
        { className: "mc-detail" },
        h(
          "button",
          { className: "mc-back", onClick: () => this.setState({ detail: null }) },
          "\u2190 All plugins"
        ),
        h(
          "div",
          { className: `mc-card${enabled ? "" : " mc-card-off"}` },
          h(
            "div",
            { className: "mc-head" },
            h(
              "div",
              { className: "mc-head-l" },
              statusDot(enabled, Api.Plugins.isRunning(name)),
              h(
                "div",
                null,
                h(
                  "div",
                  { className: "mc-name" },
                  name,
                  plugin.version ? h("span", { className: "mc-ver" }, `v${plugin.version}`) : null
                ),
                plugin.authors?.length ? h(
                  "div",
                  { className: "mc-author" },
                  `by ${plugin.authors.join(", ")}`
                ) : null
              )
            ),
            this.renderToggle(name, enabled)
          ),
          h(
            "div",
            { className: "mc-desc", style: { minHeight: 0 } },
            plugin.description ?? "No description"
          )
        ),
        hasSettings ? h(
          "div",
          { className: "mc-card", style: { marginTop: 12 } },
          h(
            "div",
            { className: "mc-name", style: { fontSize: 14 } },
            "Settings"
          ),
          this.renderSettingsBody(name, plugin)
        ) : null
      );
    }
    render() {
      const { query, filter, detail } = this.state;
      const detailRow = detail ? pluginRows().find((r) => r.name === detail) : void 0;
      if (detailRow) {
        return h(
          "div",
          { style: S.page },
          h("h2", { style: S.h2 }, "Plugins"),
          this.renderDetail(detailRow.name, detailRow.plugin, detailRow.enabled)
        );
      }
      const q = query.trim().toLowerCase();
      const rows = pluginRows().filter(({ name, plugin, enabled }) => {
        if (filter === "on" && !enabled) return false;
        if (filter === "off" && enabled) return false;
        if (!q) return true;
        return name.toLowerCase().includes(q) || name.replace(/[A-Z]/g, (c) => ` ${c.toLowerCase()}`).includes(q) || (plugin.description ?? "").toLowerCase().includes(q);
      }).sort((a, b) => a.name.localeCompare(b.name));
      const all = pluginRows();
      const counts = {
        all: all.length,
        on: all.filter((r) => r.enabled).length,
        off: all.filter((r) => !r.enabled).length
      };
      const chips = [
        ["all", "All"],
        ["on", "Enabled"],
        ["off", "Disabled"]
      ];
      return h(
        "div",
        { style: S.page },
        h("h2", { style: S.h2 }, "Plugins"),
        h(
          "div",
          { style: { ...S.muted, marginBottom: 10 } },
          `${pluginRows().length} plugins \xB7 click a card for details & settings`
        ),
        h("input", {
          style: S.input,
          placeholder: "Search plugins\u2026",
          value: query,
          onChange: (e) => this.setState({ query: e.target.value })
        }),
        h(
          "div",
          { className: "mc-filters" },
          chips.map(
            ([value, label]) => h(
              "button",
              {
                key: value,
                className: `mc-chip${filter === value ? " on" : ""}`,
                onClick: () => this.setState({ filter: value })
              },
              label,
              h("span", { className: "mc-count" }, String(counts[value]))
            )
          )
        ),
        rows.length === 0 ? h("div", { style: S.muted }, "No plugins match.") : h(
          "div",
          { className: "mc-grid" },
          rows.map(
            ({ name, plugin, enabled, running }) => h(
              "div",
              {
                key: name,
                className: `mc-card mc-card-link${enabled ? "" : " mc-card-off"}`,
                role: "button",
                tabIndex: 0,
                title: plugin.description ?? name,
                onClick: () => this.openDetail(name, plugin),
                onKeyDown: (e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault?.();
                    this.openDetail(name, plugin);
                  }
                }
              },
              h(
                "div",
                { className: "mc-head" },
                h(
                  "div",
                  { className: "mc-head-l" },
                  statusDot(enabled, running),
                  h(
                    "div",
                    null,
                    h(
                      "div",
                      { className: "mc-name" },
                      name,
                      plugin.version ? h("span", { className: "mc-ver" }, `v${plugin.version}`) : null
                    ),
                    plugin.authors?.length ? h(
                      "div",
                      { className: "mc-author" },
                      `by ${plugin.authors.join(", ")}`
                    ) : null
                  )
                ),
                this.renderToggle(name, enabled)
              ),
              h(
                "div",
                { className: "mc-desc" },
                plugin.description ?? "No description"
              )
            )
          )
        )
      );
    }
  };
  var ThemesPanel = class extends PanelBase {
    state = {
      themes: null,
      enabled: [...Themes.enabled],
      quickCss: Themes.quickCss,
      customCss: "",
      saved: false
    };
    offs = [];
    componentDidMount() {
      ensurePluginsCss();
      void Themes.list().then((themes) => this.setState({ themes }));
      void Settings.get("theme.customCss", "").then(
        (v) => this.setState({ customCss: v ?? "" })
      );
      this.offs = [
        Events.on(
          "theme.changed",
          () => this.setState({ enabled: [...Themes.enabled] })
        ),
        Events.on("settings.changed", () => this.setState({ saved: false }))
      ];
    }
    componentWillUnmount() {
      this.offs.forEach((off) => off());
      this.offs = [];
    }
    renderThemeToggle(name, isEnabled) {
      return h(
        "button",
        {
          className: `mc-toggle${isEnabled ? " on" : ""}`,
          role: "switch",
          "aria-checked": isEnabled,
          title: isEnabled ? "Disable theme" : "Enable theme",
          onClick: () => void Themes.toggle(name).then(() => this.setState({ enabled: [...Themes.enabled] })).catch((e) => log17.error("theme toggle failed", e))
        },
        h("span", null)
      );
    }
    render() {
      const { themes, enabled, quickCss, customCss, saved } = this.state;
      const setCustomCss = (v) => this.setState({ customCss: v });
      return h(
        "div",
        { style: S.page },
        h("h2", { style: S.h2 }, "Themes"),
        h(
          "div",
          { className: "mc-warn" },
          h("strong", null, "Theme performance: "),
          "Themes and custom CSS can cause lag. The most common cause is the ",
          h("code", null, ":has()"),
          " operator \u2014 avoid it in custom CSS and poorly-written themes."
        ),
        h(
          "div",
          { className: "mc-grid" },
          themes === null ? h("div", { style: S.muted }, "Loading\u2026") : themes.length === 0 ? h("div", { style: S.muted }, "No themes installed.") : themes.map((t) => {
            const isEnabled = enabled.includes(t.name);
            return h(
              "div",
              {
                key: t.name,
                className: `mc-card${isEnabled ? "" : " mc-card-off"}`
              },
              h(
                "div",
                { className: "mc-head" },
                h(
                  "div",
                  { className: "mc-head-l" },
                  statusDot(isEnabled, isEnabled),
                  h(
                    "div",
                    null,
                    h(
                      "div",
                      { className: "mc-name" },
                      t.label ?? t.name,
                      t.version ? h("span", { className: "mc-ver" }, `v${t.version}`) : null
                    ),
                    t.authors?.length ? h(
                      "div",
                      { className: "mc-author" },
                      `by ${t.authors.join(", ")}`
                    ) : null
                  )
                ),
                this.renderThemeToggle(t.name, isEnabled)
              ),
              h(
                "div",
                { className: "mc-desc", style: { minHeight: 0 } },
                t.description ?? t.name
              )
            );
          })
        ),
        h(
          "div",
          { className: "mc-card", style: { marginTop: 16 } },
          h(
            "div",
            { className: "mc-head", style: { marginBottom: 8 } },
            h("div", { className: "mc-name", style: { fontSize: 14 } }, "Custom CSS"),
            h(
              "div",
              { className: "mc-head-l" },
              h(
                "button",
                {
                  className: `mc-toggle${quickCss ? " on" : ""}`,
                  role: "switch",
                  "aria-checked": quickCss,
                  title: quickCss ? "Disable custom CSS" : "Enable custom CSS",
                  onClick: () => {
                    const next = !quickCss;
                    this.setState({ quickCss: next });
                    void Themes.setQuickCssEnabled(next);
                  }
                },
                h("span", null)
              ),
              h(
                "button",
                {
                  style: S.btn,
                  onClick: () => {
                    void Themes.setCustomCss(customCss).then(
                      () => this.setState({ saved: true })
                    );
                  }
                },
                saved ? "Saved \u2713" : "Save"
              )
            )
          ),
          h("textarea", {
            style: { ...S.input, minHeight: 140, resize: "vertical" },
            value: customCss,
            placeholder: "/* custom css */",
            onChange: (e) => setCustomCss(e.target.value)
          }),
          h(
            "div",
            { className: "mc-desc", style: { minHeight: 0, marginTop: 6 } },
            `${customCss.split("\n").length} lines \xB7 ${customCss.length} chars \xB7 saved instantly on Save`
          )
        )
      );
    }
  };

  // src/plugins/settings-ui.ts
  var log18 = Logger.get("settings-plugin");
  var SETTINGS_PLUGIN_NAME = "MonocordSettings";
  var LAYOUT_HOOK = "__monocord_buildSettingsLayout";
  var ROOT_KEY = "monocord_main";
  var LAYOUT_TYPES = {
    SECTION: 1,
    SIDEBAR_ITEM: 2,
    PANEL: 3,
    CATEGORY: 5,
    CUSTOM: 19
  };
  var active = false;
  var loggedFirstCall = false;
  function iconPath(d) {
    return () => h(
      "svg",
      {
        width: 20,
        height: 20,
        viewBox: "0 0 24 24",
        fill: "currentColor",
        "aria-hidden": true
      },
      h("path", { d })
    );
  }
  var Icons = {
    overview: iconPath(
      "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20zm1 15h-2v-6h2v6zm0-8h-2V7h2v2z"
    ),
    plugins: iconPath(
      "M4 4h7v7H4V4zm9 0h7v7h-7V4zM4 13h7v7H4v-7zm10 2h5v3h-5v-3zm-2-2h3v7h-3v-7z"
    ),
    themes: iconPath(
      "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20zm4.5 6.5-3 5.5h-3l3-5.5h3zm-9 0 3 5.5H7.5l-3-5.5h3z"
    )
  };
  function buildEntry(options) {
    const { key, title, panelTitle = title, Component, Icon } = options;
    const panel = {
      key: `${key}_panel`,
      type: LAYOUT_TYPES.PANEL,
      useTitle: () => panelTitle,
      buildLayout: () => [
        {
          key: `${key}_category`,
          type: LAYOUT_TYPES.CATEGORY,
          buildLayout: () => [
            {
              key: `${key}_custom`,
              type: LAYOUT_TYPES.CUSTOM,
              Component,
              useSearchTerms: () => [title]
            }
          ]
        }
      ]
    };
    return {
      key,
      type: LAYOUT_TYPES.SIDEBAR_ITEM,
      useTitle: () => title,
      icon: () => Icon(),
      buildLayout: () => [panel]
    };
  }
  var SECTION_KEY = "monocord_section";
  function buildSection() {
    return {
      key: SECTION_KEY,
      type: LAYOUT_TYPES.SECTION,
      useTitle: () => "Monocord",
      buildLayout: () => [
        buildEntry({
          key: ROOT_KEY,
          title: "Monocord",
          panelTitle: "Monocord",
          Component: OverviewPanel,
          Icon: Icons.overview
        }),
        buildEntry({
          key: "monocord_plugins",
          title: "Plugins",
          Component: PluginsPanel,
          Icon: Icons.plugins
        }),
        buildEntry({
          key: "monocord_themes",
          title: "Themes",
          Component: ThemesPanel,
          Icon: Icons.themes
        })
      ]
    };
  }
  function layoutHook(builder) {
    const layout = builder.buildLayout();
    if (!active) return layout;
    if (!loggedFirstCall) {
      loggedFirstCall = true;
      log18.info("settings layout hook invoked", {
        key: String(builder.key ?? "<none>")
      });
    }
    if (builder.key !== "$Root" || !Array.isArray(layout)) return layout;
    if (layout.some((node) => node?.key === SECTION_KEY)) {
      return layout;
    }
    const section = buildSection();
    const billing = layout.findIndex(
      (node) => node?.key === "billing_section"
    );
    const user = layout.findIndex(
      (node) => node?.key === "user_section"
    );
    const insertAt = billing !== -1 ? billing : user;
    layout.splice(insertAt === -1 ? layout.length : insertAt, 0, section);
    log18.info("settings layout injected", { insertAt });
    return layout;
  }
  var SettingsUIPlugin = {
    name: SETTINGS_PLUGIN_NAME,
    description: "Injects the Monocord section into Discord settings.",
    authors: ["Monocord"],
    version: "0.1.0",
    start() {
      if (active) return;
      active = true;
      window[LAYOUT_HOOK] = layoutHook;
      try {
        Webpack.patchFactories({
          plugin: SETTINGS_PLUGIN_NAME,
          find: ".buildLayout().map",
          replacement: {
            match: /([A-Za-z_$][\w$]*)\.buildLayout\(\)(?=\.map)/g,
            replace: `(0,window.${LAYOUT_HOOK})($1)`
          }
        });
        log18.info("settings layout factory patch installed");
      } catch (e) {
        log18.error("failed to install settings layout factory patch", e);
      }
      void ensureReact().then(() => logReactInfo()).catch((e) => log18.error("React not available for settings UI", e));
    },
    stop() {
      active = false;
    }
  };

  // src/renderer/index.ts
  var log19 = Logger.get("renderer");
  function evaluatePlugin(code) {
    const moduleObj = { exports: {} };
    const requireShim = (id) => {
      if (id === "monocord/api" || id === "monocord") return Api;
      throw new Error(`plugin require() is not supported for "${id}"`);
    };
    const fn = new Function(
      "Monocord",
      "module",
      "exports",
      "require",
      code
    );
    fn(Api, moduleObj, moduleObj.exports, requireShim);
    const exported = moduleObj.exports;
    if (exported && typeof exported === "object" && "default" in exported) {
      return exported;
    }
    return exported;
  }
  async function loadExternalPlugins() {
    const bridge5 = window.monocord;
    if (!bridge5) return;
    let manifests;
    try {
      manifests = await bridge5.invoke("plugins-list");
    } catch (e) {
      log19.warn("plugins-list IPC failed", e);
      return;
    }
    for (const manifest of manifests ?? []) {
      try {
        const result = await bridge5.invoke("plugin-get", manifest.name);
        if (!result?.code) continue;
        const plugin = evaluatePlugin(result.code);
        Api.Plugins.register(plugin);
        log19.info("loaded external plugin", { name: manifest.name });
      } catch (e) {
        log19.error(`failed to load external plugin ${manifest.name}`, e);
        Events.emit("plugin.error", { name: manifest.name, phase: "load", error: e });
      }
    }
  }
  async function main() {
    const bridge5 = window.monocord;
    if (bridge5) {
      Settings.useBackend(new IpcBackend(bridge5));
      setLogSink((level, scope, message) => {
        void bridge5.invoke("log", level, scope, message).catch(() => {
        });
      });
      setThemeBridge({
        listThemes: () => bridge5.invoke("themes-list"),
        getThemeCss: (name) => bridge5.invoke("theme-get", name)
      });
    } else {
      log19.warn("no monocord bridge \u2014 running without IPC persistence");
    }
    log19.info("renderer entry executing", { bridge: Boolean(bridge5) });
    const ready = await Api.Webpack.waitReady(6e4);
    if (!ready) {
      log19.error("webpack runtime never appeared \u2014 plugins disabled");
      return;
    }
    registerExamples();
    registerExtras();
    registerUxExtras();
    registerMods();
    registerChat3();
    registerSocial3();
    registerSystem3();
    registerVencordLate();
    registerVencordLast();
    try {
      Api.Plugins.register(CustomRPC);
    } catch (e) {
      log19.warn("CustomRPC already registered", e);
    }
    try {
      Api.Plugins.register(SettingsUIPlugin);
    } catch (e) {
      log19.warn("settings UI plugin already registered", e);
    }
    await loadExternalPlugins();
    const EAGER = /* @__PURE__ */ new Set([
      SETTINGS_PLUGIN_NAME,
      "CustomRPC",
      "NoTrack",
      "MessageLogger",
      "NoTypingIndicator"
    ]);
    for (const name of Api.Plugins.names()) {
      if (EAGER.has(name)) continue;
      const p = Api.Plugins.get(name);
      if (p) p.defer = true;
    }
    await Api.Plugins.startEnabled();
    if (!Api.Plugins.isEnabled(SETTINGS_PLUGIN_NAME)) {
      await Api.Plugins.enable(SETTINGS_PLUGIN_NAME).catch(
        (e) => log19.error("failed to enable settings UI", e)
      );
    }
    await Themes.restore();
    Events.emit("framework.ready", { version: Api.version });
    log19.info("framework ready");
    const ric = window.requestIdleCallback ?? ((cb) => setTimeout(cb, 200));
    ric(() => void Api.Plugins.startDeferred());
  }
  void main().catch((e) => log19.error("renderer bootstrap failed", e));
})();
