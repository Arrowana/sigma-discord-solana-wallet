export async function startTryCloudflare(port: number): Promise<string> {
  const cloudflared = Bun.spawn(
    ["cloudflared", "tunnel", "--url", `http://127.0.0.1:${port}`],
    {
      stdout: "pipe",
      stderr: "pipe",
    },
  );

  const stdoutPromise = scanForTryCloudflareUrl(cloudflared.stdout);
  const stderrPromise = scanForTryCloudflareUrl(cloudflared.stderr);
  const exitPromise = cloudflared.exited.then((code) => {
    throw new Error(`cloudflared exited early with code ${code}`);
  });

  try {
    return await Promise.race([stdoutPromise, stderrPromise, exitPromise]);
  } catch (error) {
    throw new Error(
      `failed to start trycloudflare tunnel: ${(error as Error).message}. Install cloudflared or set PUBLIC_INTERACTIONS_URL manually.`,
    );
  }
}

async function scanForTryCloudflareUrl(stream: ReadableStream<Uint8Array> | null): Promise<string> {
  if (!stream) {
    return await new Promise(() => {});
  }

  const reader = stream.getReader();
  const decoder = new TextDecoder();
  let buffer = "";

  while (true) {
    const { done, value } = await reader.read();
    if (done) {
      break;
    }
    buffer += decoder.decode(value, { stream: true });
    const match = buffer.match(/https:\/\/[a-z0-9-]+\.trycloudflare\.com/i);
    if (match) {
      return match[0];
    }
  }

  throw new Error("no trycloudflare URL found in cloudflared output");
}

