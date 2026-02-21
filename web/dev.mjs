import { resolve, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
process.chdir(__dirname);

const { createServer } = await import("vite");
const server = await createServer({
  configFile: resolve(__dirname, "vite.config.ts"),
  server: { port: parseInt(process.argv[2] || "5173"), host: true },
});
await server.listen();
server.printUrls();
