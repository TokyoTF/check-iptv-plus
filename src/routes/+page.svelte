<script lang="ts">
    import "../app.css";
    import { save } from "@tauri-apps/plugin-dialog";
    import { writeTextFile } from "@tauri-apps/plugin-fs";
    import { invoke } from "@tauri-apps/api/core";
    import { getCurrentWindow } from "@tauri-apps/api/window";
    import { onMount } from "svelte";

    const appWindow = getCurrentWindow();
    const isTauri = typeof window !== "undefined" && ("__TAURI_INTERNALS__" in window || "__TAURI__" in window);

    // Real status check. In Tauri it goes through Rust (no CORS, real HTTP
    // codes). In the browser it falls back to a CORS GET (may mark some
    // live streams dead — Tauri build is the source of truth).
    async function probeChannel(
        url: string,
    ): Promise<{ live: boolean; elapsed: number }> {
        const start = performance.now();
        if (isTauri) {
            const [status] = await invoke<[number, number]>("check_url", {
                url,
                timeoutSecs: settings.timeoutSecs,
            });
            return {
                live: status >= 200 && status < 400,
                elapsed: Math.round(performance.now() - start),
            };
        }
        const response = await fetch(url, {
            method: "GET",
            signal: AbortSignal.timeout(settings.timeoutSecs * 1000),
        });
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        // A 200 with an empty/error body is not a playable stream.
        const blob = await response.blob();
        if (blob.size === 0) throw new Error("empty body");
        return { live: true, elapsed: Math.round(performance.now() - start) };
    }

    interface Channel {
        url: string;
        status: "pending" | "live" | "dead" | "checking";
        responseTime?: number;
    }

    let channels = $state<Channel[]>([]);
    let fileName = $state("");
    let isChecking = $state(false);
    let isLoadingUrl = $state(false);
    let m3uUrl = $state("");
    let loadError = $state("");
    let activeTab = $state<"file" | "url">("file");
    let fileInput: HTMLInputElement;
    let abortController: AbortController | null = null;
    let lastIndex = $state(0);
    let toast = $state<{
        message: string;
        type: "info" | "success" | "error";
    } | null>(null);
    let toastTimeout: ReturnType<typeof setTimeout> | null = null;

    function showToast(
        message: string,
        type: "info" | "success" | "error" = "info",
    ) {
        if (toastTimeout) clearTimeout(toastTimeout);
        toast = { message, type };
        toastTimeout = setTimeout(() => {
            toast = null;
        }, 3000);
    }

    function parseM3u(content: string): string[] {
        const lines = content.split("\n");
        const urls: string[] = [];
        for (let i = 0; i < lines.length; i++) {
            const line = lines[i].trim();
            if (
                line === "" ||
                line.startsWith("#EXTM3U") ||
                line.startsWith("#EXTINF") ||
                line.startsWith("#EXTGRP") ||
                line.startsWith("#EXTVLCOPT") ||
                line.startsWith("#PLAYLIST")
            ) {
                continue;
            }
            if (line.startsWith("http://") || line.startsWith("https://")) {
                urls.push(line);
            }
        }
        return urls;
    }

    function openFile() {
        fileInput?.click();
    }

    function handleFileChange(event: Event) {
        const input = event.target as HTMLInputElement;
        const file = input.files?.[0];
        if (!file) return;

        fileName = file.name;
        m3uUrl = "";
        loadError = "";
        lastIndex = 0;
        const reader = new FileReader();
        reader.onload = (e) => {
            const content = e.target?.result as string;
            const urls = parseM3u(content);
            channels = urls.map((url) => ({
                url,
                status: "pending" as const,
            }));
        };
        reader.readAsText(file);
    }

    async function loadFromUrl() {
        if (!m3uUrl.trim()) return;
        loadError = "";
        isLoadingUrl = true;

        try {
            const response = await fetch(m3uUrl.trim());
            if (!response.ok) throw new Error(`HTTP ${response.status}`);
            const content = await response.text();
            const urls = parseM3u(content);
            if (urls.length === 0) {
                loadError = "No URLs found in the m3u file";
            } else {
                fileName = "";
                if (fileInput) fileInput.value = "";
                lastIndex = 0;
                channels = urls.map((url) => ({
                    url,
                    status: "pending" as const,
                }));
            }
        } catch (e) {
            loadError = `Load error: ${e instanceof Error ? e.message : "unknown"}`;
        } finally {
            isLoadingUrl = false;
        }
    }

    async function checkChannels() {
        if (channels.length === 0) return;
        isChecking = true;
        showToast("Checking channels...", "info");
        abortController = new AbortController();
        const signal = abortController.signal;

        for (let i = lastIndex; i < channels.length; i++) {
            if (signal.aborted) break;

            lastIndex = i;
            channels[i].status = "checking";
            channels = [...channels];

            try {
                const { live, elapsed } = await probeChannel(channels[i].url);
                if (signal.aborted) break;

                channels[i].status = live ? "live" : "dead";
                channels[i].responseTime = elapsed;
            } catch {
                if (signal.aborted) break;
                channels[i].status = "dead";
            }

            channels = [...channels];
        }

        if (!signal.aborted) {
            lastIndex = channels.length;
            showToast(`Done! ${liveCount} live, ${deadCount} dead`, "success");
        }

        isChecking = false;
        abortController = null;
    }

    function cancelCheck() {
        abortController?.abort();
        abortController = null;
        isChecking = false;
        showToast("Check cancelled", "error");
    }

    async function exportResults() {
        const liveUrls = channels
            .filter((c) => c.status === "live")
            .map((c) => c.url)
            .join("\n");

        const path = await save({
            defaultPath: "iptv-results.txt",
            filters: [{ name: "Text", extensions: ["txt"] }],
        });

        if (path) {
            await writeTextFile(path, liveUrls);
        }
    }

    function clearAll() {
        if (isChecking) cancelCheck();
        channels = [];
        fileName = "";
        m3uUrl = "";
        loadError = "";
        lastIndex = 0;
        if (fileInput) fileInput.value = "";
    }

    let liveCount = $derived(
        channels.filter((c) => c.status === "live").length,
    );
    let deadCount = $derived(
        channels.filter((c) => c.status === "dead").length,
    );
    let pendingCount = $derived(
        channels.filter(
            (c) => c.status === "pending" || c.status === "checking",
        ).length,
    );

    interface Settings {
        timeoutSecs: number;
    }

    const DEFAULT_SETTINGS: Settings = { timeoutSecs: 8 };

    function loadSettings(): Settings {
        try {
            const raw = localStorage.getItem("check-iptv-settings");
            if (!raw) return { ...DEFAULT_SETTINGS };
            const parsed = JSON.parse(raw) as Partial<Settings>;
            const timeoutSecs = Number(parsed.timeoutSecs);
            return {
                timeoutSecs:
                    Number.isFinite(timeoutSecs) &&
                    timeoutSecs >= 2 &&
                    timeoutSecs <= 60
                        ? Math.round(timeoutSecs)
                        : DEFAULT_SETTINGS.timeoutSecs,
            };
        } catch {
            return { ...DEFAULT_SETTINGS };
        }
    }

    let settings = $state<Settings>(
        typeof localStorage !== "undefined"
            ? loadSettings()
            : { ...DEFAULT_SETTINGS },
    );
    let showSettings = $state(false);
    let showAbout = $state(false);
    let draftTimeout = $state(DEFAULT_SETTINGS.timeoutSecs);
    let scrollContainer = $state<HTMLElement | null>(null);
    let showScrollTop = $state(false);

    function scrollToTop() {
        scrollContainer?.scrollTo({ top: 0, behavior: "smooth" });
    }

    let updateStatus = $state<
        "idle" | "checking" | "available" | "latest" | "error"
    >("idle");
    let updateVersion = $state("");
    let appVersion = $state("");

    onMount(async () => {
        if (!isTauri) return;
        try {
            const { getVersion } = await import("@tauri-apps/api/app");
            appVersion = await getVersion();
        } catch {
            /* version unavailable */
        }
        // Auto-check on startup (silent: no toast when already latest)
        try {
            const { check } = await import("@tauri-apps/plugin-updater");
            const update = await check();
            if (update) {
                updateVersion = update.version;
                updateStatus = "available";
                showToast(`Update v${update.version} available`, "info");
            } else {
                updateStatus = "latest";
            }
        } catch {
            /* keep idle so manual retry is possible */
        }
    });

    async function checkForUpdates() {
        if (updateStatus === "checking") return;
        if (!isTauri) {
            showToast("Updates are only available in the desktop app", "info");
            return;
        }
        updateStatus = "checking";
        try {
            const { check } = await import("@tauri-apps/plugin-updater");
            const update = await check();
            if (update) {
                updateVersion = update.version;
                updateStatus = "available";
            } else {
                updateStatus = "latest";
            }
        } catch {
            updateStatus = "error";
        }
    }

    async function installUpdate() {
        if (updateStatus !== "available") return;
        updateStatus = "checking";
        try {
            const { check } = await import("@tauri-apps/plugin-updater");
            const { relaunch } = await import("@tauri-apps/plugin-process");
            const update = await check();
            if (update) {
                await update.downloadAndInstall();
                await relaunch();
            } else {
                updateStatus = "latest";
            }
        } catch {
            updateStatus = "error";
            showToast("Update failed", "error");
        }
    }

    function openSettings() {
        draftTimeout = settings.timeoutSecs;
        showSettings = true;
    }

    function saveSettings() {
        const v = Math.round(Number(draftTimeout));
        settings.timeoutSecs = Number.isFinite(v)
            ? Math.min(60, Math.max(2, v))
            : DEFAULT_SETTINGS.timeoutSecs;
        try {
            localStorage.setItem(
                "check-iptv-settings",
                JSON.stringify(settings),
            );
        } catch {
            /* storage unavailable */
        }
        showSettings = false;
        showToast(`Timeout set to ${settings.timeoutSecs}s`, "success");
    }

    function resetSettings() {
        settings = { ...DEFAULT_SETTINGS };
        draftTimeout = DEFAULT_SETTINGS.timeoutSecs;
        try {
            localStorage.setItem(
                "check-iptv-settings",
                JSON.stringify(settings),
            );
        } catch {
            /* storage unavailable */
        }
    }

    let contextMenu = $state<{ x: number; y: number; index: number } | null>(
        null,
    );

    function openContextMenu(e: MouseEvent, index: number) {
        e.preventDefault();
        const menuWidth = 180;
        const menuHeight = 150;
        contextMenu = {
            x: Math.min(e.clientX, window.innerWidth - menuWidth),
            y: Math.min(e.clientY, window.innerHeight - menuHeight),
            index,
        };
    }

    function closeContextMenu() {
        contextMenu = null;
    }

    async function copyUrl(index: number) {
        try {
            await navigator.clipboard.writeText(channels[index].url);
            showToast("URL copied", "success");
        } catch {
            showToast("Copy failed", "error");
        }
        closeContextMenu();
    }

    async function openInBrowser(index: number) {
        const { openUrl } = await import("@tauri-apps/plugin-opener");
        await openUrl(channels[index].url);
        closeContextMenu();
    }

    async function recheckOne(index: number) {
        closeContextMenu();
        if (isChecking) return;
        channels[index].status = "checking";
        channels = [...channels];
        try {
            const { live, elapsed } = await probeChannel(channels[index].url);
            channels[index].status = live ? "live" : "dead";
            channels[index].responseTime = elapsed;
        } catch {
            channels[index].status = "dead";
        }
        channels = [...channels];
    }

    function removeOne(index: number) {
        channels = channels.filter((_, i) => i !== index);
        if (lastIndex > channels.length) lastIndex = channels.length;
        closeContextMenu();
    }
</script>

<div
    class="h-screen flex flex-col bg-[#f3f3f3] font-[Segoe_UI,-apple-system,BlinkMacSystemFont,sans-serif] select-none overflow-hidden"
>
    <!-- Sticky Titlebar -->
    <header
        data-tauri-drag-region
        class="h-11 bg-white/80 backdrop-blur-xl border-b border-black/5 flex items-center shrink-0"
    >
        <div class="flex items-center gap-3 px-4" data-tauri-drag-region>
            <img
                src="logo.webp"
                loading="eager"
                alt="Check IPTV Plus logo"
                class="h-6 w-auto"
            />
            <span class="text-xs font-semibold text-[#1a1a1a] tracking-tight"
                >Check IPTV Plus</span
            >
            <span
                class="text-xs text-[#999] bg-[#f0f0f0] px-1.5 py-0.5 font-medium"
                >v{appVersion}</span
            >
            <button
                onclick={() => {
                    showAbout = true;
                }}
                title="About"
                class="text-xs text-[#999] bg-[#f0f0f0] px-1.5 py-0.5 font-medium hover:text-[#0078d4] hover:bg-[#e5f1fd] transition-colors cursor-pointer"
            >
                About
            </button>
        </div>

        <div class="ml-auto flex items-center h-full">
            <button
                onclick={openSettings}
                aria-label="Settings"
                title="Settings"
                class="h-full px-4 flex items-center justify-center hover:bg-black/5 text-[#616161] transition-colors cursor-pointer"
            >
                <svg
                    class="w-4 h-4"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                >
                    <circle cx="12" cy="12" r="3" />
                    <path
                        d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 11-2.83 2.83l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 11-4 0v-.09a1.65 1.65 0 00-1-1.51 1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 11-2.83-2.83l.06-.06a1.65 1.65 0 00.33-1.82 1.65 1.65 0 00-1.51-1H3a2 2 0 110-4h.09a1.65 1.65 0 001.51-1 1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 112.83-2.83l.06.06a1.65 1.65 0 001.82.33h.01a1.65 1.65 0 001-1.51V3a2 2 0 114 0v.09a1.65 1.65 0 001 1.51h.01a1.65 1.65 0 001.82-.33l.06-.06a2 2 0 112.83 2.83l-.06.06a1.65 1.65 0 00-.33 1.82v.01a1.65 1.65 0 001.51 1H21a2 2 0 110 4h-.09a1.65 1.65 0 00-1.51 1z"
                    />
                </svg>
            </button>
            <button
                onclick={() => appWindow.minimize()}
                aria-label="Minimize"
                class="h-full px-4 flex items-center justify-center hover:bg-black/5 transition-colors cursor-pointer"
            >
                <svg
                    class="w-4 h-4 text-[#616161]"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                >
                    <path d="M5 12h14" />
                </svg>
            </button>
            <button
                onclick={() => appWindow.toggleMaximize()}
                aria-label="Maximize"
                class="h-full px-4 flex items-center justify-center hover:bg-black/5 transition-colors cursor-pointer"
            >
                <svg
                    class="w-4 h-4 text-[#616161]"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                >
                    <rect x="4" y="4" width="16" height="16" rx="1" />
                </svg>
            </button>
            <button
                onclick={() => appWindow.close()}
                aria-label="Close"
                class="h-full px-4 flex items-center justify-center hover:bg-[#c42b1c] hover:text-white text-[#616161] transition-colors cursor-pointer"
            >
                <svg
                    class="w-4 h-4"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                >
                    <path d="M6 6l12 12M18 6L6 18" />
                </svg>
            </button>
        </div>
    </header>

    <!-- Scrollable content -->
    <main
        bind:this={scrollContainer}
        onscroll={(e) => {
            showScrollTop = (e.currentTarget as HTMLElement).scrollTop > 300;
        }}
        class="flex-1 overflow-y-auto"
    >
        <div class="max-w-6xl mx-auto p-4 flex flex-col gap-4">
            <!-- Input card -->
            <div
                class="bg-white border border-black/[0.06] shadow-[0_1px_3px_rgba(0,0,0,0.04)]"
            >
                <!-- Tabs -->
                <div class="flex border-b border-black/[0.06]">
                    <button
                        onclick={() => {
                            activeTab = "file";
                        }}
                        class="flex items-center gap-2 px-4 py-2 text-xs font-medium border-b-2 transition-all cursor-pointer
              {activeTab === 'file'
                            ? 'border-[#0078d4] text-[#0078d4] bg-[#f0f7ff]'
                            : 'border-transparent text-[#616161] hover:text-[#1a1a1a] hover:bg-[#fafafa]'}"
                    >
                        <svg
                            class="w-3.5 h-3.5"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                        >
                            <path
                                d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"
                            />
                        </svg>
                        File
                    </button>
                    <button
                        onclick={() => {
                            activeTab = "url";
                        }}
                        class="flex items-center gap-2 px-4 py-2 text-xs font-medium border-b-2 transition-all cursor-pointer
              {activeTab === 'url'
                            ? 'border-[#0078d4] text-[#0078d4] bg-[#f0f7ff]'
                            : 'border-transparent text-[#616161] hover:text-[#1a1a1a] hover:bg-[#fafafa]'}"
                    >
                        <svg
                            class="w-3.5 h-3.5"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                        >
                            <path
                                d="M10 13a5 5 0 007.54.54l3-3a5 5 0 00-7.07-7.07l-1.72 1.71"
                            />
                            <path
                                d="M14 11a5 5 0 00-7.54-.54l-3 3a5 5 0 007.07 7.07l1.71-1.71"
                            />
                        </svg>
                        URL
                    </button>
                </div>

                <div class="p-3">
                    {#if activeTab === "file"}
                        <div class="flex items-center gap-2">
                            <button
                                onclick={openFile}
                                class="inline-flex items-center gap-1.5 px-3 py-1.5 bg-[#0078d4] text-white text-xs font-medium hover:bg-[#106ebe] active:scale-[0.98] transition-all cursor-pointer shrink-0"
                            >
                                <svg
                                    class="w-3.5 h-3.5"
                                    viewBox="0 0 24 24"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="2"
                                >
                                    <path
                                        d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"
                                    />
                                </svg>
                                Open file
                            </button>

                            <input
                                bind:this={fileInput}
                                type="file"
                                accept=".m3u,.m3u8,.txt"
                                class="hidden"
                                onchange={handleFileChange}
                            />

                            <div
                                class="flex-1 px-3 py-1.5 bg-[#f9f9f9] border border-black/[0.06] text-xs text-[#616161] truncate cursor-default"
                            >
                                {fileName || "No file selected"}
                            </div>
                        </div>
                    {:else}
                        <div class="flex items-center gap-2">
                            <input
                                bind:value={m3uUrl}
                                type="url"
                                placeholder="https://example.com/playlist.m3u"
                                class="flex-1 px-3 py-1.5 bg-[#f9f9f9] border border-black/[0.06] text-xs text-[#1a1a1a] placeholder:text-[#aaa] focus:outline-none focus:border-[#0078d4] focus:ring-2 focus:ring-[#0078d4]/10 transition-all"
                                onkeydown={(e) => {
                                    if (e.key === "Enter") loadFromUrl();
                                }}
                            />
                            <button
                                onclick={loadFromUrl}
                                disabled={isLoadingUrl || !m3uUrl.trim()}
                                class="inline-flex items-center gap-1.5 px-3 py-1.5 bg-[#0078d4] text-white text-xs font-medium hover:bg-[#106ebe] active:scale-[0.98] disabled:opacity-40 disabled:cursor-not-allowed disabled:active:scale-100 transition-all cursor-pointer shrink-0"
                            >
                                {#if isLoadingUrl}
                                    <svg
                                        class="w-3.5 h-3.5 animate-spin"
                                        viewBox="0 0 24 24"
                                        fill="none"
                                    >
                                        <circle
                                            class="opacity-25"
                                            cx="12"
                                            cy="12"
                                            r="10"
                                            stroke="currentColor"
                                            stroke-width="4"
                                        />
                                        <path
                                            class="opacity-75"
                                            fill="currentColor"
                                            d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
                                        />
                                    </svg>
                                    Loading...
                                {:else}
                                    <svg
                                        class="w-3.5 h-3.5"
                                        viewBox="0 0 24 24"
                                        fill="none"
                                        stroke="currentColor"
                                        stroke-width="2"
                                    >
                                        <path
                                            d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4M7 10l5 5 5-5M12 15V3"
                                        />
                                    </svg>
                                    Load
                                {/if}
                            </button>
                        </div>
                    {/if}

                    {#if loadError}
                        <div
                            class="mt-2 px-3 py-1.5 bg-red-50 border border-red-100 text-[#c42b1c] text-xs flex items-center gap-1.5"
                        >
                            <svg
                                class="w-3.5 h-3.5 shrink-0"
                                viewBox="0 0 24 24"
                                fill="none"
                                stroke="currentColor"
                                stroke-width="2"
                            >
                                <circle cx="12" cy="12" r="10" />
                                <path d="M15 9l-6 6M9 9l6 6" />
                            </svg>
                            {loadError}
                        </div>
                    {/if}
                </div>
            </div>
            <!-- Sticky action bar: Cancel/Check, Export, Clear + live/dead/pending/total -->
            {#if channels.length > 0}
                <div
                    class="sticky top-0 z-20 bg-white border border-black/[0.06] shadow-[0_1px_3px_rgba(0,0,0,0.04)] px-3 py-2 flex items-center gap-2"
                >
                    {#if isChecking}
                        <button
                            onclick={cancelCheck}
                            class="inline-flex items-center gap-1.5 px-3 py-1.5 bg-[#c42b1c] text-white text-xs font-medium hover:bg-[#a12019] active:scale-[0.98] transition-all cursor-pointer"
                        >
                            <svg
                                class="w-3.5 h-3.5"
                                viewBox="0 0 24 24"
                                fill="none"
                                stroke="currentColor"
                                stroke-width="2"
                            >
                                <rect
                                    x="6"
                                    y="6"
                                    width="12"
                                    height="12"
                                    rx="1"
                                />
                            </svg>
                            Cancel
                        </button>
                    {:else}
                        <button
                            onclick={checkChannels}
                            class="inline-flex items-center gap-1.5 px-3 py-1.5 bg-[#0f7b0f] text-white text-xs font-medium hover:bg-[#0a5e0a] active:scale-[0.98] transition-all cursor-pointer"
                        >
                            <svg
                                class="w-3.5 h-3.5"
                                viewBox="0 0 24 24"
                                fill="none"
                                stroke="currentColor"
                                stroke-width="2"
                            >
                                <path d="M22 11.08V12a10 10 0 11-5.93-9.14" />
                                <path d="M22 4L12 14.01l-3-3" />
                            </svg>
                            {lastIndex > 0 && lastIndex < channels.length
                                ? "Resume"
                                : "Check"}
                        </button>
                    {/if}

                    <button
                        onclick={exportResults}
                        disabled={isChecking}
                        class="inline-flex items-center gap-1.5 px-3 py-1.5 border border-black/[0.08] bg-white text-xs font-medium hover:bg-[#f5f5f5] active:scale-[0.98] disabled:opacity-40 disabled:cursor-not-allowed disabled:active:scale-100 transition-all cursor-pointer"
                    >
                        <svg
                            class="w-3.5 h-3.5"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                        >
                            <path
                                d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4M7 10l5 5 5-5M12 15V3"
                            />
                        </svg>
                        Export
                    </button>

                    <button
                        onclick={clearAll}
                        class="inline-flex items-center gap-1 px-2 py-1.5 text-xs text-[#616161] hover:bg-[#f0f0f0] active:scale-[0.98] transition-all cursor-pointer"
                    >
                        <svg
                            class="w-3.5 h-3.5"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                        >
                            <path d="M18 6L6 18M6 6l12 12" />
                        </svg>
                        Clear
                    </button>

                    <div
                        class="ml-auto flex items-center gap-3 text-xs font-medium"
                    >
                        <span class="flex items-center gap-1">
                            <span class="w-1.5 h-1.5 bg-[#0f7b0f]"></span>
                            <span class="text-[#0f7b0f]">{liveCount}</span>
                            <span class="text-[#999]">live</span>
                        </span>
                        <span class="flex items-center gap-1">
                            <span class="w-1.5 h-1.5 bg-[#c42b1c]"></span>
                            <span class="text-[#c42b1c]">{deadCount}</span>
                            <span class="text-[#999]">dead</span>
                        </span>
                        <span class="flex items-center gap-1">
                            <span class="w-1.5 h-1.5 bg-[#9e5e00]"></span>
                            <span class="text-[#9e5e00]">{pendingCount}</span>
                            <span class="text-[#999]">pending</span>
                        </span>
                        <span class="w-px h-3 bg-black/10"></span>
                        <span class="text-[#616161]"
                            >{channels.length} total</span
                        >
                    </div>
                </div>
            {/if}
            <!-- Table card -->
            {#if channels.length > 0}
                <div
                    class="flex-1 bg-white border border-black/[0.06] shadow-[0_1px_3px_rgba(0,0,0,0.04)] overflow-hidden flex flex-col min-h-[400px]"
                >
                    <div class="overflow-auto flex-1">
                        <table class="w-full text-xs">
                            <thead class="sticky top-0 z-10">
                                <tr
                                    class="bg-[#fafafa] border-b border-black/[0.06]"
                                >
                                    <th
                                        class="text-left px-5 py-3 font-semibold text-[#616161] w-10 text-xs uppercase tracking-wider"
                                        >#</th
                                    >
                                    <th
                                        class="text-left px-5 py-3 font-semibold text-[#616161] text-xs uppercase tracking-wider"
                                        >URL</th
                                    >
                                    <th
                                        class="text-left px-5 py-3 font-semibold text-[#616161] w-28 text-xs uppercase tracking-wider"
                                        >Status</th
                                    >
                                    <th
                                        class="text-left px-5 py-3 font-semibold text-[#616161] w-28 text-xs uppercase tracking-wider"
                                        >Time</th
                                    >
                                </tr>
                            </thead>
                            <tbody>
                                {#each channels as channel, i}
                                    <tr
                                        class="border-b border-black/[0.04] hover:bg-[#f8f8f8] transition-colors group cursor-default"
                                        oncontextmenu={(e) =>
                                            openContextMenu(e, i)}
                                    >
                                        <td
                                            class="px-5 py-2.5 text-[#bbb] font-medium"
                                            >{i + 1}</td
                                        >
                                        <td
                                            class="px-5 py-2.5 text-[#1a1a1a] truncate max-w-xs"
                                            title={channel.url}
                                        >
                                            {channel.url}
                                        </td>
                                        <td class="px-5 py-2.5">
                                            {#if channel.status === "live"}
                                                <span
                                                    class="inline-flex items-center gap-1.5 px-2.5 py-1 text-xs font-semibold bg-emerald-50 text-emerald-700 border border-emerald-100"
                                                >
                                                    <span
                                                        class="w-1.5 h-1.5 bg-emerald-500"
                                                    ></span>
                                                    Live
                                                </span>
                                            {:else if channel.status === "dead"}
                                                <span
                                                    class="inline-flex items-center gap-1.5 px-2.5 py-1 text-xs font-semibold bg-red-50 text-red-700 border border-red-100"
                                                >
                                                    <span
                                                        class="w-1.5 h-1.5 bg-red-500"
                                                    ></span>
                                                    Dead
                                                </span>
                                            {:else if channel.status === "checking"}
                                                <span
                                                    class="inline-flex items-center gap-1.5 px-2.5 py-1 text-xs font-semibold bg-amber-50 text-amber-700 border border-amber-100"
                                                >
                                                    <svg
                                                        class="w-3 h-3 animate-spin"
                                                        viewBox="0 0 24 24"
                                                        fill="none"
                                                    >
                                                        <circle
                                                            class="opacity-25"
                                                            cx="12"
                                                            cy="12"
                                                            r="10"
                                                            stroke="currentColor"
                                                            stroke-width="4"
                                                        />
                                                        <path
                                                            class="opacity-75"
                                                            fill="currentColor"
                                                            d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
                                                        />
                                                    </svg>
                                                    Checking
                                                </span>
                                            {:else}
                                                <span
                                                    class="inline-flex items-center gap-1.5 px-2.5 py-1 text-xs font-semibold bg-gray-100 text-gray-600 border border-gray-200"
                                                >
                                                    Pending
                                                </span>
                                            {/if}
                                        </td>
                                        <td class="px-5 py-2.5 text-[#888]">
                                            {channel.responseTime
                                                ? `${channel.responseTime}ms`
                                                : "-"}
                                        </td>
                                    </tr>
                                {/each}
                            </tbody>
                        </table>
                    </div>
                </div>
            {:else}
                <div
                    class="flex-1 flex items-center justify-center min-h-[300px]"
                >
                    <div class="text-center">
                        <div
                            class="w-20 h-20 mx-auto mb-4 bg-[#f0f0f0] flex items-center justify-center"
                        >
                            <svg
                                class="w-9 h-9 text-[#ccc]"
                                viewBox="0 0 24 24"
                                fill="none"
                                stroke="currentColor"
                                stroke-width="1.5"
                            >
                                <path
                                    d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"
                                />
                            </svg>
                        </div>
                        <p class="text-xs text-[#999] font-medium">
                            Select a file or enter a URL to get started
                        </p>
                        <p class="text-xs text-[#ccc] mt-1">
                            Supports .m3u, .m3u8 and .txt formats
                        </p>
                    </div>
                </div>
            {/if}
        </div>
    </main>

    <!-- Context menu -->
    {#if contextMenu}
        <div
            role="presentation"
            class="fixed inset-0 z-40 cursor-default"
            onclick={closeContextMenu}
            oncontextmenu={(e) => {
                e.preventDefault();
                closeContextMenu();
            }}
        ></div>
        <div
            class="fixed z-50 w-44 bg-white border border-black/[0.08] shadow-lg py-1 text-xs"
            style="left: {contextMenu.x}px; top: {contextMenu.y}px;"
            role="menu"
        >
            <button
                onclick={() => copyUrl(contextMenu!.index)}
                role="menuitem"
                class="w-full text-left px-3 py-1.5 hover:bg-[#f0f7ff] hover:text-[#0078d4] flex items-center gap-2 cursor-pointer"
            >
                Copy URL
            </button>
            <button
                onclick={() => openInBrowser(contextMenu!.index)}
                role="menuitem"
                class="w-full text-left px-3 py-1.5 hover:bg-[#f0f7ff] hover:text-[#0078d4] flex items-center gap-2 cursor-pointer"
            >
                Open in browser
            </button>
            <button
                onclick={() => recheckOne(contextMenu!.index)}
                role="menuitem"
                class="w-full text-left px-3 py-1.5 hover:bg-[#f0f7ff] hover:text-[#0078d4] flex items-center gap-2 cursor-pointer"
            >
                Recheck
            </button>
            <div class="border-t border-black/[0.06] my-1"></div>
            <button
                onclick={() => removeOne(contextMenu!.index)}
                role="menuitem"
                class="w-full text-left px-3 py-1.5 text-[#c42b1c] hover:bg-red-50 flex items-center gap-2 cursor-pointer"
            >
                Remove
            </button>
        </div>
    {/if}

    <!-- Settings modal -->
    {#if showSettings}
        <div
            role="presentation"
            class="fixed inset-0 z-50 bg-black/30 flex items-center justify-center p-4"
            onclick={() => {
                showSettings = false;
            }}
        >
            <div
                role="dialog"
                aria-modal="true"
                aria-label="Settings"
                class="bg-white shadow-xl w-full max-w-sm"
                onclick={(e) => e.stopPropagation()}
            >
                <div
                    class="flex items-center px-4 py-3 border-b border-black/[0.06]"
                >
                    <span class="text-xs font-semibold text-[#1a1a1a]"
                        >Settings</span
                    >
                    <button
                        onclick={() => {
                            showSettings = false;
                        }}
                        aria-label="Close settings"
                        class="ml-auto text-[#999] hover:text-[#1a1a1a] cursor-pointer"
                    >
                        <svg
                            class="w-4 h-4"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            ><path d="M18 6L6 18M6 6l12 12" /></svg
                        >
                    </button>
                </div>
                <div class="p-4 flex flex-col gap-3">
                    <label class="flex flex-col gap-1.5 text-xs text-[#616161]">
                        <span class="font-medium text-[#1a1a1a]"
                            >Request timeout (seconds)</span
                        >
                        <input
                            type="number"
                            min="2"
                            max="60"
                            step="1"
                            bind:value={draftTimeout}
                            class="px-3 py-1.5 bg-[#f9f9f9] border border-black/[0.08] text-xs text-[#1a1a1a] focus:outline-none focus:border-[#0078d4] focus:ring-2 focus:ring-[#0078d4]/10"
                        />
                        <span class="text-[#999]"
                            >Between 2 and 60 seconds. Applies to the next
                            check.</span
                        >
                    </label>
                </div>
                <div
                    class="flex items-center justify-end gap-2 px-4 py-3 border-t border-black/[0.06]"
                >
                    <button
                        onclick={resetSettings}
                        class="px-3 py-1.5 text-xs text-[#616161] hover:bg-[#f0f0f0] cursor-pointer"
                        >Reset</button
                    >
                    <button
                        onclick={saveSettings}
                        class="px-3 py-1.5 bg-[#0078d4] text-white text-xs font-medium hover:bg-[#106ebe] cursor-pointer"
                        >Save</button
                    >
                </div>
            </div>
        </div>
    {/if}

    <!-- About modal -->
    {#if showAbout}
        <div
            role="presentation"
            class="fixed inset-0 z-50 bg-black/30 flex items-center justify-center p-4"
            onclick={() => {
                showAbout = false;
            }}
        >
            <div
                role="dialog"
                aria-modal="true"
                aria-label="About Check IPTV Plus"
                class="bg-white shadow-xl w-full max-w-sm"
                onclick={(e) => e.stopPropagation()}
            >
                <div
                    class="flex items-center px-4 py-3 border-b border-black/[0.06]"
                >
                    <span class="text-xs font-semibold text-[#1a1a1a]"
                        >About</span
                    >
                    <button
                        onclick={() => {
                            showAbout = false;
                        }}
                        aria-label="Close about"
                        class="ml-auto text-[#999] hover:text-[#1a1a1a] cursor-pointer"
                    >
                        <svg
                            class="w-4 h-4"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            ><path d="M18 6L6 18M6 6l12 12" /></svg
                        >
                    </button>
                </div>
                <div class="p-4 flex flex-col gap-2 text-xs text-[#616161]">
                    <div class="flex items-center gap-2">
                        <img
                            src="logo.webp"
                            loading="eager"
                            alt="Check IPTV Plus logo"
                            class="h-8 w-auto"
                        />
                        <div>
                            <p class="font-semibold text-[#1a1a1a]">
                                Check IPTV Plus
                            </p>
                            <p class="text-[#999]">v{appVersion}</p>
                        </div>
                    </div>
                    <p>
                        Validates M3U / M3U8 playlists and checks which streams
                        are live, dead.
                    </p>

                    <button
                        onclick={async () => {
                            const { openUrl } =
                                await import("@tauri-apps/plugin-opener");
                            await openUrl(
                                "https://github.com/TokyoTF/check-iptv-plus",
                            );
                        }}
                        class="self-start inline-flex items-center gap-1.5 text-[#0078d4] hover:underline cursor-pointer"
                    >
                        <svg
                            class="w-3.5 h-3.5"
                            viewBox="0 0 24 24"
                            fill="currentColor"
                        >
                            <path
                                d="M12 .5C5.65.5.5 5.65.5 12c0 5.08 3.29 9.39 7.86 10.91.58.11.79-.25.79-.55v-2.15c-3.2.7-3.87-1.36-3.87-1.36-.52-1.33-1.28-1.68-1.28-1.68-1.04-.71.08-.7.08-.7 1.15.08 1.76 1.18 1.76 1.18 1.03 1.76 2.69 1.25 3.35.96.1-.75.4-1.25.72-1.54-2.55-.29-5.23-1.28-5.23-5.68 0-1.26.45-2.28 1.18-3.09-.12-.29-.51-1.46.11-3.05 0 0 .96-.31 3.15 1.18a10.9 10.9 0 015.74 0c2.19-1.49 3.15-1.18 3.15-1.18.62 1.59.23 2.76.11 3.05.74.81 1.18 1.83 1.18 3.09 0 4.41-2.69 5.38-5.25 5.67.41.35.77 1.05.77 2.12v3.14c0 .3.21.67.8.55A11.51 11.51 0 0023.5 12C23.5 5.65 18.35.5 12 .5z"
                            />
                        </svg>
                        github.com/TokyoTF/check-iptv-plus
                    </button>
                    <button
                        onclick={async () => {
                            const { openUrl } =
                                await import("@tauri-apps/plugin-opener");
                            await openUrl(
                                "https://github.com/sponsors/TokyoTF",
                            );
                        }}
                        class="self-start inline-flex items-center gap-1.5 text-[#db61a2] hover:underline cursor-pointer"
                    >
                        <svg
                            class="w-3.5 h-3.5"
                            viewBox="0 0 24 24"
                            fill="currentColor"
                        >
                            <path
                                d="M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z"
                            />
                        </svg>
                        Sponsor @TokyoTF
                    </button>
                    <div class="border-t border-black/[0.06] pt-3 mt-1">
                        {#if updateStatus === "available"}
                            <button
                                onclick={installUpdate}
                                class="w-full px-3 py-1.5 bg-[#0f7b0f] text-white text-xs font-medium hover:bg-[#0a5e0a] cursor-pointer"
                            >
                                Install update v{updateVersion} and restart
                            </button>
                        {:else}
                            <button
                                onclick={checkForUpdates}
                                disabled={updateStatus === "checking"}
                                class="w-full px-3 py-1.5 border border-black/[0.08] bg-white text-xs font-medium hover:bg-[#f5f5f5] disabled:opacity-40 disabled:cursor-not-allowed cursor-pointer"
                            >
                                {updateStatus === "checking"
                                    ? "Checking for updates..."
                                    : "Check for updates"}
                            </button>
                            {#if updateStatus === "latest"}
                                <p class="text-[#0f7b0f] mt-1.5">
                                    You're on the latest version.
                                </p>
                            {:else if updateStatus === "error"}
                                <p class="text-[#c42b1c] mt-1.5">
                                    Couldn't check for updates. Try again later.
                                </p>
                            {/if}
                        {/if}
                    </div>
                </div>
            </div>
        </div>
    {/if}

    <!-- Scroll to top -->
    {#if showScrollTop}
        <button
            onclick={scrollToTop}
            aria-label="Scroll to top"
            title="Scroll to top"
            class="fixed bottom-4 right-4 z-40 w-9 h-9 bg-white border border-black/[0.08] shadow-lg flex items-center justify-center text-[#616161] hover:text-[#0078d4] hover:border-[#0078d4]/30 active:scale-95 transition-all cursor-pointer"
        >
            <svg
                class="w-4 h-4"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
            >
                <path d="M12 19V5M5 12l7-7 7 7" />
            </svg>
        </button>
    {/if}

    <!-- Toast popup -->
    {#if toast}
        <div class="fixed bottom-4 left-4 z-50 animate-[slideIn_0.2s_ease-out]">
            <div
                class="flex items-center gap-2 px-4 py-2.5 text-xs font-medium shadow-lg border
          {toast.type === 'success'
                    ? 'bg-emerald-50 text-emerald-700 border-emerald-200'
                    : ''}
          {toast.type === 'error'
                    ? 'bg-red-50 text-red-700 border-red-200'
                    : ''}
          {toast.type === 'info'
                    ? 'bg-white text-[#1a1a1a] border-black/[0.06]'
                    : ''}"
            >
                {#if toast.type === "success"}
                    <svg
                        class="w-4 h-4 text-emerald-500"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                    >
                        <path d="M22 11.08V12a10 10 0 11-5.93-9.14" />
                        <path d="M22 4L12 14.01l-3-3" />
                    </svg>
                {:else if toast.type === "error"}
                    <svg
                        class="w-4 h-4 text-red-500"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                    >
                        <circle cx="12" cy="12" r="10" />
                        <path d="M15 9l-6 6M9 9l6 6" />
                    </svg>
                {:else}
                    <svg
                        class="w-4 h-4 text-[#0078d4]"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                    >
                        <circle cx="12" cy="12" r="10" />
                        <path d="M12 16v-4M12 8h.01" />
                    </svg>
                {/if}
                {toast.message}
            </div>
        </div>
    {/if}
</div>
