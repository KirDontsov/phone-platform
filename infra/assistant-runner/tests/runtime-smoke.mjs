import { nativeProbe } from "./native-probe.mjs";
try {
  console.log(JSON.stringify({ status: "passed", ...(await nativeProbe()) }));
} catch {
  console.error('{"status":"failed","error":"assistant_native_smoke_failed"}');
  process.exitCode = 1;
}
