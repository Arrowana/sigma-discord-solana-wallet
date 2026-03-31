import { startTryCloudflare } from "./cloudflare";

const port = Number(process.env.PORT ?? "3000");
const tunnelUrl = await startTryCloudflare(port);

console.log(`Forwarding http://127.0.0.1:${port}`);
console.log(`Tunnel URL: ${tunnelUrl}`);
console.log(`Interactions endpoint: ${tunnelUrl.replace(/\/$/, "")}/interactions`);
console.log("Leave this process running while your local bot server is up.");
