import fs from "node:fs/promises";
import path from "node:path";
import { build } from "esbuild";
import "./patch-moq.mjs";

const root = path.resolve("..");
const output = path.join(root, "web");
const entries = ["moq-player", "flv-player", "evaluation"];

await fs.rm(output, { recursive: true, force: true });
await fs.cp(path.resolve("static"), output, { recursive: true });
await fs.mkdir(path.join(output, "generated"), { recursive: true });

await Promise.all(
  entries.map((name) =>
    build({
      entryPoints: [path.resolve(`${name}.ts`)],
      outfile: path.join(output, "generated", `${name}.js`),
      bundle: true,
      format: "esm",
      minify: true,
      banner: {
        js: `// Generated from web-src/${name}.ts by npm run build. Do not edit.`,
      },
    }),
  ),
);
