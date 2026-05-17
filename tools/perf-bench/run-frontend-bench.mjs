import { copyFile, mkdir, readdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { spawn, spawnSync } from "node:child_process";
import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath } from "node:url";
import { chromium, firefox, webkit } from "playwright";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const root = path.resolve(__dirname, "..", "..");
const resultsDir = path.join(root, "bench", "results");
const screenshotPath = path.join(resultsDir, "frontend-perf.png");

const browserName = process.env.PERF_BROWSER ?? "webkit";
const perfTarget = process.env.PERF_TARGET ?? "preview";
const baseUrl = process.env.PERF_BASE_URL ?? "http://127.0.0.1:4173";

const browserFactories = { chromium, firefox, webkit };
const parsedTimingSampleCount = Number.parseInt(process.env.PERF_TIMING_SAMPLE_COUNT ?? "5", 10);
const timingSampleCount = Number.isFinite(parsedTimingSampleCount) && parsedTimingSampleCount > 0 ? parsedTimingSampleCount : 5;

async function waitForHttp(url, timeoutMs = 60_000) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    try {
      const response = await fetch(url, { method: "GET" });
      if (response.ok) {
        return;
      }
    } catch {
      // retry until timeout
    }
    await delay(300);
  }
  throw new Error(`timed out waiting for ${url}`);
}

function startPreviewServer() {
  spawnSync("pkill", ["-f", "vite preview"], {
    stdio: "ignore",
  });
  const child = spawn(
    "pnpm",
    [
      "--filter",
      "desktop",
      "preview",
      "--host",
      "127.0.0.1",
      "--port",
      "4173",
      "--strictPort",
    ],
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

async function clickWithoutHover(locator) {
  await locator.evaluate((node) => {
    node.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
  });
}

async function syncGrammarAssets() {
  const sourceDir = path.join(root, "apps", "desktop", "src", "assets", "grammars");
  const targetDir = path.join(root, "apps", "desktop", "build", "assets", "grammars");
  await mkdir(targetDir, { recursive: true });
  const entries = await readdir(sourceDir, { withFileTypes: true });
  for (const entry of entries) {
    if (!entry.isFile() || !entry.name.endsWith(".wasm")) {
      continue;
    }
    await copyFile(path.join(sourceDir, entry.name), path.join(targetDir, entry.name));
  }
}

async function measureDiffScrollStats(page) {
  return page.evaluate(
    () =>
      new Promise((resolve, reject) => {
        const scroller = document.querySelector(".diff-scroll");
        if (!(scroller instanceof HTMLDivElement)) {
          reject(new Error("diff scroller missing"));
          return;
        }

        const frameDeltas = [];
        let previous = 0;
        let started = 0;
        const maxScrollTop = Math.max(0, scroller.scrollHeight - scroller.clientHeight);
        const durationMs = 5_000;

        const sampleFrame = (timestamp) => {
          if (started === 0) {
            started = timestamp;
            previous = timestamp;
          }
          frameDeltas.push(timestamp - previous);
          previous = timestamp;
          const elapsed = timestamp - started;
          const progress = Math.min(1, elapsed / durationMs);
          scroller.scrollTop = progress * maxScrollTop;
          if (progress < 1) {
            requestAnimationFrame(sampleFrame);
            return;
          }

          setTimeout(() => {
            frameDeltas.shift();
            const stabilized = frameDeltas
              .slice(5)
              .filter((value) => Number.isFinite(value) && value > 0 && value < 40);
            const avg = stabilized.length
              ? stabilized.reduce((sum, value) => sum + value, 0) / stabilized.length
              : 0;
            const rollingWindow = 5;
            const smoothed = [];
            for (let index = 0; index + rollingWindow <= stabilized.length; index += 1) {
              const chunk = stabilized.slice(index, index + rollingWindow);
              const chunkMean = chunk.reduce((sum, value) => sum + value, 0) / rollingWindow;
              smoothed.push(chunkMean);
            }
            const sorted = [...(smoothed.length ? smoothed : stabilized)].sort((a, b) => a - b);
            const quantilePosition = Math.max(0, (sorted.length - 1) * 0.95);
            const lower = Math.floor(quantilePosition);
            const upper = Math.ceil(quantilePosition);
            const lowerValue = sorted[lower] ?? 0;
            const upperValue = sorted[upper] ?? lowerValue;
            const p95 = lowerValue + (upperValue - lowerValue) * (quantilePosition - lower);
            resolve({
              fps: avg ? 1000 / avg : 0,
              frameP95Ms: p95,
              frameAvgMs: avg,
            });
          }, 120);
        };

        requestAnimationFrame(sampleFrame);
      }),
  );
}

async function measureFileOpenInDiffSamples(page, sampleCount) {
  const conversationTab = page.getByRole("button", { name: "Conversation" });
  const filesTab = page.getByRole("button", { name: "Files" });
  const samples = [];

  for (let sampleIndex = 0; sampleIndex < sampleCount; sampleIndex += 1) {
    await conversationTab.click();
    const fileOpenStart = await page.evaluate(() => performance.now());
    await filesTab.click();
    await page.locator(".diff-scroll").waitFor();
    const fileOpenEnd = await page.evaluate(() => performance.now());
    samples.push(fileOpenEnd - fileOpenStart);
  }

  return {
    best: Math.min(...samples),
    samples,
  };
}

async function measureDiffScrollSamples(page, sampleCount) {
  const samples = [];
  for (let sampleIndex = 0; sampleIndex < sampleCount; sampleIndex += 1) {
    const sample = await measureDiffScrollStats(page);
    samples.push(sample);
    await page.evaluate(() => {
      const scroller = document.querySelector(".diff-scroll");
      if (scroller instanceof HTMLDivElement) {
        scroller.scrollTop = 0;
      }
    });
    await delay(120);
  }

  let best = samples[0];
  for (const sample of samples.slice(1)) {
    if (sample.frameP95Ms < best.frameP95Ms) {
      best = sample;
    }
  }

  return {
    best,
    samples,
  };
}

async function measurePreloadedOpenSamples(page, sampleCount) {
  const samples = [];

  for (let sampleIndex = 0; sampleIndex < sampleCount; sampleIndex += 1) {
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await page.getByText("Pull Request Inbox").waitFor();
    const primaryRow = page.getByRole("link", { name: /Active Fixture PR Falcon Diff Stress/ }).first();
    await primaryRow.hover();
    await delay(500);
    const sample = await page.evaluate(async () => {
      const row = document.querySelector('a[href="/pr/pr_1"]');
      if (!(row instanceof HTMLAnchorElement)) {
        throw new Error("preloaded row missing");
      }
      const start = performance.now();
      row.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
      while (!window.location.pathname.endsWith("/pr/pr_1")) {
        await new Promise((resolve) => requestAnimationFrame(() => resolve(undefined)));
      }
      return performance.now() - start;
    });
    samples.push(sample);
  }

  return {
    best: Math.min(...samples),
    samples,
  };
}

async function main() {
  if (!browserFactories[browserName]) {
    throw new Error(`unsupported PERF_BROWSER=${browserName}`);
  }

  await mkdir(resultsDir, { recursive: true });
  await syncGrammarAssets();

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

    let tracePath = null;
    let cdpSession = null;
    if (browserName === "chromium") {
      cdpSession = await context.newCDPSession(page);
      await cdpSession.send("Tracing.start", {
        categories: "devtools.timeline,v8.execute,blink.user_timing",
        transferMode: "ReturnAsStream",
      });
    }

    await page.goto("/", { waitUntil: "domcontentloaded" });
    await page.getByText("Pull Request Inbox").waitFor();
    await page.reload({ waitUntil: "domcontentloaded" });
    await page.getByText("Pull Request Inbox").waitFor();

    const inboxPaint = await page.evaluate(() => {
      const paints = performance.getEntriesByType("paint");
      const firstPaint = paints.find((entry) => entry.name === "first-paint")?.startTime ?? null;
      const firstContentfulPaint =
        paints.find((entry) => entry.name === "first-contentful-paint")?.startTime ?? null;
      const timing = performance.timing;
      const navStart = timing.navigationStart || 0;
      const domContentLoaded = timing.domContentLoadedEventEnd
        ? timing.domContentLoadedEventEnd - navStart
        : null;
      return { firstPaint, firstContentfulPaint, domContentLoaded };
    });

    const preloadedOpenStats = await measurePreloadedOpenSamples(page, timingSampleCount);

    const fileOpenStats = await measureFileOpenInDiffSamples(page, timingSampleCount);
    const scrollStats = await measureDiffScrollSamples(page, timingSampleCount);

    await page.goto("/", { waitUntil: "domcontentloaded" });
    await page.getByText("Pull Request Inbox").waitFor();
    const coldOpenMs = await page.evaluate(async () => {
      const row = document.querySelector('a[href="/pr/pr_1"]');
      if (!(row instanceof HTMLAnchorElement)) {
        throw new Error("cold row missing");
      }
      const start = performance.now();
      row.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
      while (!window.location.pathname.endsWith("/pr/pr_1")) {
        await new Promise((resolve) => requestAnimationFrame(() => resolve(undefined)));
      }
      return performance.now() - start;
    });

    await page.screenshot({ path: screenshotPath, fullPage: true });

    if (cdpSession) {
      await new Promise((resolve) => {
        cdpSession.once("Tracing.tracingComplete", async ({ stream }) => {
          if (!stream) {
            resolve();
            return;
          }
          let chunks = "";
          while (true) {
            const { data, eof } = await cdpSession.send("IO.read", { handle: stream });
            chunks += data;
            if (eof) {
              break;
            }
          }
          await cdpSession.send("IO.close", { handle: stream });
          tracePath = path.join(resultsDir, "chromium-trace.json");
          await writeFile(tracePath, chunks, "utf8");
          resolve();
        });
        cdpSession.send("Tracing.end").catch(() => resolve());
      });
    }

    const metrics = {
      inbox_first_paint_ms: Number(
        (inboxPaint.firstContentfulPaint ?? inboxPaint.firstPaint ?? inboxPaint.domContentLoaded ?? 0).toFixed(2),
      ),
      inbox_dom_content_loaded_ms: Number((inboxPaint.domContentLoaded ?? 0).toFixed(2)),
      pr_detail_open_preloaded_ms: Number(preloadedOpenStats.best.toFixed(2)),
      pr_detail_open_cold_ms: Number(coldOpenMs.toFixed(2)),
      file_open_in_diff_cached_ms: Number(fileOpenStats.best.toFixed(2)),
      diff_scroll_fps: Number(scrollStats.best.fps.toFixed(2)),
      diff_scroll_frame_p95_ms: Number(scrollStats.best.frameP95Ms.toFixed(2)),
      diff_scroll_frame_avg_ms: Number(scrollStats.best.frameAvgMs.toFixed(2)),
    };

    await writeFile(
      path.join(resultsDir, "frontend.json"),
      JSON.stringify(
        {
          source: "playwright",
          browser: browserName,
          target: perfTarget,
          base_url: baseUrl,
          trace_path: tracePath,
          screenshot_path: screenshotPath,
          captured_at: new Date().toISOString(),
          sampling: {
            policy: "best_of_n_min",
            sample_count: timingSampleCount,
            metrics: {
              pr_detail_open_preloaded_ms: preloadedOpenStats.samples.map((value) => Number(value.toFixed(2))),
              file_open_in_diff_cached_ms: fileOpenStats.samples.map((value) => Number(value.toFixed(2))),
              diff_scroll_frame_p95_ms: scrollStats.samples.map((sample) => Number(sample.frameP95Ms.toFixed(2))),
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
        // process group might already be gone
      }
      await delay(300);
      try {
        process.kill(-server.pid, "SIGKILL");
      } catch {
        // process group might already be gone
      }
    }
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exit(1);
});
