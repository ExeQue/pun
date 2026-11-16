const projectRoot = process.argv[2] || process.cwd();
const configPath = process.argv[3];

const prettierEntry = require.resolve("prettier", { paths: [projectRoot] });

const prettierIndex = require("url").pathToFileURL(
    require("path").join(require("path").dirname(prettierEntry), "index.mjs"),
).href;

// Work that Prettier and its plugins redo for every file, though it only depends on what they loaded, kept by patching
// their source as Node loads it. The hooks go once the first file is formatted, by when all of it is loaded.
const stopPatching = patchModules([
    // Prettier normalizes the options of every format call, and of every embedded language in it, from scratch: it
    // rebuilds the option list of all its plugins and a validation schema per option. Both only depend on the loaded
    // plugins, which Prettier caches, so they are memoized.
    {
        matches: (href) => href === prettierIndex,
        edits: [
            [
                `const supportOptions = getSupportInfo({
    plugins: options7.plugins,
    showDeprecated: true
  }).options;`,
                "const supportOptions = punSupport(options7.plugins);",
            ],
            [
                "function optionInfosToSchemas(",
                `var punSupportOptions = [];
function punSupport(plugins) {
  for (const [key, value] of punSupportOptions) {
    if (key.length === plugins.length && key.every((plugin, i) => plugin === plugins[i])) return value;
  }
  const value = getSupportInfo({ plugins, showDeprecated: true }).options;
  punSupportOptions.push([[...plugins], value]);
  return value;
}
var punSchemas = new WeakMap();
function optionInfosToSchemas(optionInfos, options) {
  if (options.isCLI) return punOptionInfosToSchemas(optionInfos, options);
  let schemas = punSchemas.get(optionInfos);
  if (!schemas) {
    schemas = punOptionInfosToSchemas(optionInfos, options);
    punSchemas.set(optionInfos, schemas);
  }
  return schemas;
}
function punOptionInfosToSchemas(`,
            ],
        ],
    },
    // prettier-plugin-tailwindcss resolves Tailwind from a file's directory up to three times per file, and caches the
    // answer only for directories whose path is at least as long as Tailwind's package directory, which leaves out
    // resources/views and its shorter subdirectories. Caching it for the file's own directory as well gives the same
    // answer, from the same expiring cache, without resolving again.
    {
        matches: (href) => href.includes("/prettier-plugin-tailwindcss/dist/"),
        edits: [
            [
                "\tif (pkgDir) cacheForDirs(resolvedModCache, baseDir, result, pkgDir, makeKey);\n",
                "\tif (pkgDir) cacheForDirs(resolvedModCache, baseDir, result, pkgDir, makeKey);\n" +
                    "\tif (pkgDir && resolvedModCache.get(makeKey(baseDir)) === void 0) resolvedModCache.set(makeKey(baseDir), result);\n",
            ],
        ],
    },
]);

const prettier = require(prettierEntry);

const bundledOptions = require(configPath);

function resolvePlugins(plugins) {
    if (!Array.isArray(plugins)) {
        return plugins;
    }

    return plugins.map((plugin) =>
        typeof plugin === "string"
            ? require.resolve(plugin, { paths: [projectRoot] })
            : plugin,
    );
}

function resolveOptionPlugins(options) {
    if (options.plugins) {
        options.plugins = resolvePlugins(options.plugins);
    }

    if (Array.isArray(options.overrides)) {
        for (const override of options.overrides) {
            if (override.options && override.options.plugins) {
                override.options.plugins = resolvePlugins(
                    override.options.plugins,
                );
            }
        }
    }

    return options;
}

process.stdin.setEncoding("utf-8");

let buffer = "";
let queue = Promise.resolve();
let cacheFlushed = false;

process.stdin.on("data", function (chunk) {
    buffer += chunk;

    let newlineIndex;

    while ((newlineIndex = buffer.indexOf("\n")) !== -1) {
        const line = buffer.slice(0, newlineIndex);
        buffer = buffer.slice(newlineIndex + 1);

        if (line.trim() === "") {
            continue;
        }

        queue = queue.then(() => handleMessage(line));
    }
});

async function handleMessage(input) {
    try {
        const { path: filepath, content } = JSON.parse(input);

        const resolved = filepath.trim();

        const options = resolveOptionPlugins({ ...bundledOptions });

        const formatted = await prettier.format(content, {
            ...options,
            filepath: resolved,
        });

        process.stdout.write(
            `[PINT_PRETTIER_WORKER_START]${formatted}[PINT_PRETTIER_WORKER_END]`,
        );

        // Pun kills its workers, so Node never writes the compile cache (NODE_COMPILE_CACHE) on exit. Writing it
        // after the first file keeps the code compiled for Prettier and its plugins for the next run's workers.
        if (!cacheFlushed) {
            cacheFlushed = true;
            stopPatching();
            try {
                require("module").flushCompileCache?.();
            } catch {}
        }
    } catch (error) {
        process.stderr.write(`${error.stack || error.message}\n`);
    }
}

// Loads each module that `patch.matches` with every `[from, to]` replacement of its `edits`, or as is when its source
// lacks one of them, until the returned function runs. Without module.registerHooks (Bun, an older Node) nothing
// changes.
function patchModules(patches) {
    const module = require("module");
    if (typeof module.registerHooks !== "function") {
        return () => {};
    }
    // A Node that still marks the hooks experimental warns on stderr, which Pun reads as a failed file.
    const emitWarning = process.emitWarning;
    process.emitWarning = () => {};
    try {
        const hooks = module.registerHooks({
            load(href, context, nextLoad) {
                const loaded = nextLoad(href, context);
                const patch = patches.find((patch) => patch.matches(href));
                if (!patch) {
                    return loaded;
                }
                let source =
                    typeof loaded.source === "string" ? loaded.source : new TextDecoder().decode(loaded.source);
                if (!patch.edits.every(([from]) => source.includes(from))) {
                    return loaded;
                }
                for (const [from, to] of patch.edits) {
                    source = source.replace(from, () => to);
                }
                return { ...loaded, source };
            },
        });
        return () => hooks.deregister();
    } catch {
        return () => {};
    } finally {
        process.emitWarning = emitWarning;
    }
}
