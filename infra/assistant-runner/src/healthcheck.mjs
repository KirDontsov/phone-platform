try {
  const response = await fetch("http://127.0.0.1:8091/health", {
    signal: AbortSignal.timeout(2500),
  });
  const data = await response.json();
  if (!response.ok || data.healthy !== true || data.codex_version !== "0.167.2")
    process.exitCode = 1;
} catch {
  process.exitCode = 1;
}
