import fs from "node:fs/promises";
import path from "node:path";
import crypto from "node:crypto";
import { SafeError, requireValue, uuid } from "./common.mjs";
export async function privateDirectory(directory) {
  await fs.mkdir(directory, { recursive: true, mode: 0o700 });
  const stat = await fs.lstat(directory);
  requireValue(
    stat.isDirectory() && !stat.isSymbolicLink(),
    "unsafe_runtime_directory",
  );
  await fs.chmod(directory, 0o700);
}
export async function writePrivate(file, value) {
  const temporary = file + "." + crypto.randomUUID() + ".tmp";
  try {
    await fs.writeFile(temporary, value, { mode: 0o600, flag: "wx" });
    await fs.rename(temporary, file);
    await fs.chmod(file, 0o600);
  } finally {
    await fs.rm(temporary, { force: true });
  }
}
export async function readPrivate(file, max = 131072) {
  try {
    const stat = await fs.lstat(file);
    requireValue(
      stat.isFile() &&
        !stat.isSymbolicLink() &&
        (stat.mode & 0o077) === 0 &&
        stat.size <= max,
      "unsafe_runtime_file",
    );
    return await fs.readFile(file);
  } catch (error) {
    if (error.code === "ENOENT") return null;
    throw error;
  }
}
export class Storage {
  constructor(root, temp) {
    this.root = root;
    this.temp = temp;
    this.locks = new Map();
  }
  async init() {
    await privateDirectory(this.root);
    await privateDirectory(this.temp);
    for (const name of await fs.readdir(this.temp)) {
      if (/^turn-|^auth-/.test(name))
        await fs.rm(path.join(this.temp, name), {
          recursive: true,
          force: true,
        });
    }
  }
  async lock(org, fn) {
    uuid(org);
    const previous = this.locks.get(org) || Promise.resolve();
    let release;
    const next = new Promise((resolve) => {
      release = resolve;
    });
    this.locks.set(org, next);
    await previous.catch(() => {});
    try {
      return await fn();
    } finally {
      release();
      if (this.locks.get(org) === next) this.locks.delete(org);
    }
  }
  async org(org) {
    const directory = path.join(this.root, uuid(org));
    await privateDirectory(directory);
    return directory;
  }
  async read(org) {
    const directory = await this.org(org);
    const raw = await readPrivate(path.join(directory, "state.json"));
    if (!raw) return { generation: 0, authenticated: false, auth_mode: null };
    let data;
    try {
      data = JSON.parse(raw);
    } catch {
      throw new SafeError("invalid_auth_state");
    }
    requireValue(
      Number.isSafeInteger(data.generation) && data.generation >= 0,
      "invalid_auth_state",
    );
    return data;
  }
  async write(org, state) {
    await writePrivate(
      path.join(await this.org(org), "state.json"),
      JSON.stringify(state),
    );
  }
  async auth(org) {
    return readPrivate(path.join(await this.org(org), "auth.json"));
  }
  async removeAuth(org) {
    await fs.rm(path.join(await this.org(org), "auth.json"), { force: true });
  }
  async promote(org, home) {
    const raw = await readPrivate(path.join(home, "auth.json"));
    requireValue(raw, "codex_auth_missing");
    try {
      requireValue(
        JSON.parse(raw) && typeof JSON.parse(raw) === "object",
        "invalid_auth_file",
      );
    } catch {
      throw new SafeError("invalid_auth_file");
    }
    await writePrivate(path.join(await this.org(org), "auth.json"), raw);
  }
  async context(prefix) {
    const directory = await fs.mkdtemp(path.join(this.temp, prefix + "-"));
    await fs.chmod(directory, 0o700);
    const home = path.join(directory, "home"),
      work = path.join(directory, "work");
    await privateDirectory(home);
    await privateDirectory(work);
    return {
      directory,
      home,
      work,
      cleanup: () => fs.rm(directory, { recursive: true, force: true }),
    };
  }
}
