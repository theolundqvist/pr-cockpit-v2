import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { spawn, spawnSync } from "node:child_process";
import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath } from "node:url";
import { chromium, firefox, webkit } from "playwright";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const root = path.resolve(__dirname, "..", "..");
const resultsDir = path.join(root, "bench", "results");
const browserName = process.env.PERF_BROWSER ?? "webkit";
const perfTarget = process.env.PERF_TARGET ?? "preview";
const baseUrl = process.env.PERF_BASE_URL ?? "http://127.0.0.1:4173";
const browserFactories = { chromium, firefox, webkit };
const sampleCount = Number.parseInt(process.env.PERF_TIMING_SAMPLE_COUNT ?? "5", 10);
const effectiveSampleCount = Number.isFinite(sampleCount) && sampleCount > 0 ? sampleCount : 5;

function startPreviewServer() {
  spawnSync("pkill", ["-f", "vite preview"], { stdio: "ignore" });
  const child = spawn(
    "pnpm",
    ["--filter", "desktop", "preview", "--host", "127.0.0.1", "--port", "4173", "--strictPort"],
    {
      cwd: root,
      stdio: "ignore",
      shell: false,
      detached: true,
    },
  );
  child.unref();
  return child;
}

async function waitForHttp(url, timeoutMs = 60_000) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    try {
      const response = await fetch(url, { method: "GET" });
      if (response.ok) {
        return;
      }
    } catch {
      // retry
    }
    await delay(300);
  }
  throw new Error(`timed out waiting for ${url}`);
}

async function ensurePaletteClosed(page) {
  await page.evaluate(() => {
    const root = document.querySelector("[data-testid='command-palette-root']");
    if (!root?.classList.contains("is-open")) {
      return;
    }
    window.dispatchEvent(
      new KeyboardEvent("keydown", {
        key: "Escape",
        bubbles: true,
        cancelable: true,
      }),
    );
  });
}

async function measureOpenSample(page) {
  return page.evaluate(async () => {
    const isMac = navigator.platform.toLowerCase().includes("mac");
    const start = performance.now();
    window.dispatchEvent(
      new KeyboardEvent("keydown", {
        key: "k",
        ctrlKey: !isMac,
        metaKey: isMac,
        bubbles: true,
        cancelable: true,
      }),
    );
    const deadline = performance.now() + 5_000;
    while (performance.now() < deadline) {
      const root = document.querySelector("[data-testid='command-palette-root']");
      const input = document.querySelector("[data-testid='command-palette-search']");
      if (
        root instanceof HTMLDivElement &&
        root.classList.contains("is-open") &&
        input instanceof HTMLInputElement &&
        document.activeElement === input
      ) {
        return performance.now() - start;
      }
      await new Promise((resolve) => requestAnimationFrame(() => resolve(undefined)));
    }
    throw new Error("timed out waiting for command palette open");
  });
}

async function measureResultSample(page, query, expectedTitle) {
  return page.evaluate(
    async ({ queryText, expected }) => {
      const input = document.querySelector("[data-testid='command-palette-search']");
      if (!(input instanceof HTMLInputElement)) {
        throw new Error("palette search input missing");
      }
      input.focus();
      input.value = "";
      input.dispatchEvent(new Event("input", { bubbles: true }));
      await new Promise((resolve) => requestAnimationFrame(() => resolve(undefined)));
      const start = performance.now();
      input.value = queryText;
      input.dispatchEvent(new Event("input", { bubbles: true }));
      const deadline = performance.now() + 5_000;
      while (performance.now() < deadline) {
        const matches = [...document.querySelectorAll("button[data-testid^='command-palette-item-']")];
        const hit = matches.find((node) => node.textContent?.toLowerCase().includes(expected));
        if (hit) {
          return performance.now() - start;
        }
        await new Promise((resolve) => requestAnimationFrame(() => resolve(undefined)));
      }
      throw new Error("timed out waiting for filtered command result");
    },
    { queryText: query, expected: expectedTitle.toLowerCase() },
  );
}

async function main() {
  if (!browserFactories[browserName]) {
    throw new Error(`unsupported PERF_BROWSER=${browserName}`);
  }

  await mkdir(resultsDir, { recursive: true });

  let server = null;
  if (perfTarget === "preview") {
    server = startPreviewServer();
    await waitForHttp(baseUrl);
  }

  let browser;
  try {
    browser = await browserFactories[browserName].launch({ headless: true });
    const context = await browser.newContext({ baseURL: baseUrl });
    const page = await context.newPage();
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await page.getByText("Pull Request Inbox").waitFor();

    const openSamples = [];
    const resultSamples = [];
    for (let index = 0; index < effectiveSampleCount; index += 1) {
      await ensurePaletteClosed(page);
      const openMs = await measureOpenSample(page);
      openSamples.push(openMs);
      const resultMs = await measureResultSample(page, "open inbox", "Open inbox");
      resultSamples.push(resultMs);
      await ensurePaletteClosed(page);
      await delay(40);
    }

    const metrics = {
      command_palette_open_ms: Number(Math.min(...openSamples).toFixed(2)),
      command_palette_result_ms: Number(Math.min(...resultSamples).toFixed(2)),
    };

    await writeFile(
      path.join(resultsDir, "command-palette.json"),
      JSON.stringify(
        {
          source: "playwright",
          browser: browserName,
          target: perfTarget,
          base_url: baseUrl,
          captured_at: new Date().toISOString(),
          sampling: {
            policy: "best_of_n_min",
            sample_count: effectiveSampleCount,
            metrics: {
              command_palette_open_ms: openSamples.map((value) => Number(value.toFixed(2))),
              command_palette_result_ms: resultSamples.map((value) => Number(value.toFixed(2))),
            },
          },
          metrics,
        },
        null,
        2,
      ) + "\n",
      "utf8",
    );

    for (const [name, value] of Object.entries(metrics)) {
      console.log(`${name}=${value}`);
    }
  } finally {
    if (browser) {
      await browser.close();
    }
    if (server) {
      try {
        process.kill(-server.pid, "SIGTERM");
      } catch {
        // no-op
      }
      await delay(300);
      try {
        process.kill(-server.pid, "SIGKILL");
      } catch {
        // no-op
      }
    }
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exit(1);
});
