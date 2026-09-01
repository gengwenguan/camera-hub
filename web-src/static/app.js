(() => {
    "use strict";

    const $ = (id) => document.getElementById(id);
    const ui = {
        connection: $("hubConnection"),
        refresh: $("refreshButton"),
        logout: $("logoutButton"),
        updated: $("lastUpdated"),
        online: $("onlineMetric"),
        total: $("totalMetric"),
        recording: $("recordingMetric"),
        bytes: $("bytesMetric"),
        uptime: $("uptimeMetric"),
        hostIdentity: $("hostIdentity"),
        hostCpu: $("hostCpu"),
        hostLoad: $("hostLoad"),
        hostMemory: $("hostMemory"),
        hostMemoryDetail: $("hostMemoryDetail"),
        hostProcess: $("hostProcess"),
        hostDisk: $("hostDisk"),
        hostDiskDetail: $("hostDiskDetail"),
        devices: $("deviceGrid"),
        settingsForm: $("settingsForm"),
        settingsStatus: $("settingsStatus"),
        aiEnabled: $("aiEnabled"),
        aiInterval: $("aiInterval"),
        aiThreshold: $("aiThreshold"),
        aiMinPersonArea: $("aiMinPersonArea"),
        aiCooldown: $("aiCooldown"),
        aiSnapshotMaxCount: $("aiSnapshotMaxCount"),
        aiSnapshotQuality: $("aiSnapshotQuality"),
        aiRuntimeStatus: $("aiRuntimeStatus"),
        recordEnabled: $("recordEnabled"),
        segmentSeconds: $("segmentSeconds"),
        retainDays: $("retainDays"),
        maxGb: $("maxGb"),
        reloadSettings: $("reloadSettings"),
        saveSettings: $("saveSettings"),
        voiceStatus: $("voiceStatus"),
        voiceState: $("voiceState"),
        voiceDetected: $("voiceDetected"),
        voiceAudioLevel: $("voiceAudioLevel"),
        voiceLastKeyword: $("voiceLastKeyword"),
        voiceLastError: $("voiceLastError"),
        voiceForm: $("voiceForm"),
        voiceEnabled: $("voiceEnabled"),
        voiceCaptureDevice: $("voiceCaptureDevice"),
        voicePlaybackDevice: $("voicePlaybackDevice"),
        voicePlaybackVolume: $("voicePlaybackVolume"),
        voicePlaybackVolumeValue: $("voicePlaybackVolumeValue"),
        voiceRequestTimeout: $("voiceRequestTimeout"),
        voiceGlobalCooldown: $("voiceGlobalCooldown"),
        voiceFailureReply: $("voiceFailureReply"),
        voiceCommandList: $("voiceCommandList"),
        voiceEventList: $("voiceEventList"),
        addVoiceCommand: $("addVoiceCommand"),
        reloadVoice: $("reloadVoice"),
        saveVoice: $("saveVoice"),
        qqStatus: $("qqStatus"),
        qqState: $("qqState"),
        qqBotName: $("qqBotName"),
        qqConnectedAt: $("qqConnectedAt"),
        qqSentCount: $("qqSentCount"),
        qqLastError: $("qqLastError"),
        qqForm: $("qqForm"),
        qqEnabled: $("qqEnabled"),
        qqAppId: $("qqAppId"),
        qqAppSecret: $("qqAppSecret"),
        qqSecretState: $("qqSecretState"),
        qqDefaultGroup: $("qqDefaultGroup"),
        qqGroupList: $("qqGroupList"),
        reloadQq: $("reloadQq"),
        saveQq: $("saveQq"),
        clearQqSecret: $("clearQqSecret"),
        qqPushTokenState: $("qqPushTokenState"),
        qqPushEndpoint: $("qqPushEndpoint"),
        qqPushToken: $("qqPushToken"),
        rotateQqPushToken: $("rotateQqPushToken"),
        copyQqPushToken: $("copyQqPushToken"),
        qqTestForm: $("qqTestForm"),
        qqTestTarget: $("qqTestTarget"),
        qqTestMessage: $("qqTestMessage"),
        sendQqTest: $("sendQqTest"),
        liveDeviceSelect: $("liveDeviceSelect"),
        livePlayer: $("livePlayer"),
        liveStatus: $("liveStatus"),
        liveClock: $("liveClock"),
        startSmoothLive: $("startSmoothLive"),
        startFlvLive: $("startFlvLive"),
        startLowLive: $("startLowLive"),
        startMoqLive: $("startMoqLive"),
        stopLive: $("stopLive"),
        moqPlayer: $("moqPlayer"),
        deviceSelect: $("deviceSelect"),
        dateSelect: $("dateSelect"),
        playbackSpeed: $("playbackSpeed"),
        summary: $("recordSummary"),
        records: $("recordTable"),
        playerBox: $("recordPlayerBox"),
        player: $("recordPlayer"),
        playerTitle: $("recordPlayerTitle"),
        playerStatus: $("recordPlayerStatus"),
        recordDownload: $("recordDownload"),
        timeline: $("recordTimeline"),
        timelinePosition: $("timelinePosition"),
        timelineTip: $("timelineTip"),
        photoDeviceSelect: $("photoDeviceSelect"),
        refreshPhotos: $("refreshPhotos"),
        selectAllPhotos: $("selectAllPhotos"),
        clearPhotoSelection: $("clearPhotoSelection"),
        deleteSelectedPhotos: $("deleteSelectedPhotos"),
        clearPhotos: $("clearPhotos"),
        photoSummary: $("photoSummary"),
        photoGrid: $("photoGrid"),
        photoEmpty: $("photoEmpty"),
        photoLightbox: $("photoLightbox"),
        photoPreview: $("photoPreview"),
        downloadPhoto: $("downloadPhoto"),
        deletePhoto: $("deletePhoto"),
        closePhoto: $("closePhoto"),
        toast: $("toast"),
    };
    const state = {
        devices: [],
        media: [],
        days: [],
        records: [],
        photos: [],
        photoSelection: new Set(),
        currentPhoto: "",
        device: "",
        date: "",
        view: "overview",
        currentRecord: "",
        segmentSeconds: 600,
        timelinePreview: null,
        voiceConfig: null,
        voiceDirty: false,
        voiceBusy: false,
        qqConfig: null,
        qqDirty: false,
        qqBusy: false,
        busy: false,
        settingsDirty: false,
        toastTimer: 0,
    };

    async function api(path, options = {}) {
        const response = await fetch(path, { cache: "no-store", ...options });
        if (response.status === 401) {
            location.replace("/login");
            throw new Error("authentication required");
        }
        const text = await response.text();
        let body = null;
        try {
            body = text ? JSON.parse(text) : null;
        } catch (_) {
            body = null;
        }
        if (!response.ok) {
            throw new Error(body && body.error ? body.error : `HTTP ${response.status}`);
        }
        return body;
    }

    ui.logout.addEventListener("click", async () => {
        try {
            await fetch("/api/v1/auth/logout", {
                method: "POST",
                cache: "no-store",
            });
        } finally {
            location.replace("/login");
        }
    });

    async function refreshAll(silent = false) {
        if (state.busy) return;
        state.busy = true;
        ui.refresh.disabled = true;
        setConnection("idle", "正在刷新");
        try {
            const [deviceBody, mediaBody, health, system, settings, ai] = await Promise.all([
                api("/api/v1/devices"),
                api("/api/v1/media/status"),
                api("/health"),
                api("/api/v1/system/status"),
                api("/api/v1/settings"),
                api("/api/v1/ai/status"),
            ]);
            state.devices = Array.isArray(deviceBody.devices) ? deviceBody.devices : [];
            state.media = Array.isArray(mediaBody.devices) ? mediaBody.devices : [];
            if (!state.devices.some((item) => item.device_id === state.device)) {
                state.device = state.devices[0] ? state.devices[0].device_id : "";
                state.date = "";
                state.photoSelection.clear();
            }
            renderMetrics(health);
            renderHost(system);
            renderDevices();
            renderDeviceSelect();
            renderSettings(settings, ai);
            await loadDays();
            if (state.view === "photos") await loadPhotos();
            ui.updated.textContent = new Date().toLocaleTimeString("zh-CN", { hour12: false });
            setConnection("online", "服务已连接");
            if (!silent) showToast("状态已刷新");
        } catch (error) {
            handleError(error);
        } finally {
            state.busy = false;
            ui.refresh.disabled = false;
        }
    }

    function renderMetrics(health) {
        const online = state.devices.filter((item) => item.online).length;
        const recording = state.media.filter((item) => item.recording).length;
        const bytes = state.media.reduce((sum, item) => sum + Number(item.bytes || 0), 0);
        ui.online.textContent = String(online);
        ui.total.textContent = `共 ${state.devices.length} 台已注册设备`;
        ui.recording.textContent = String(recording);
        ui.bytes.textContent = formatBytes(bytes);
        ui.uptime.textContent = formatDuration(Number(health.uptime_seconds || 0));
    }

    function renderHost(system) {
        const hostLabel = system.hostname && system.hostname !== "localhost"
            ? system.hostname : "camera-hub";
        ui.hostIdentity.textContent =
            [hostLabel, system.kernel].filter(Boolean).join(" · ");
        if (system.cpu && system.cpu.valid) {
            ui.hostCpu.textContent = `${Number(system.cpu.percent).toFixed(0)}%`;
            ui.hostLoad.textContent = `${system.cpu.cores || 1} 核` +
                (system.load && system.load.valid
                    ? ` · 负载 ${Number(system.load.one).toFixed(2)}`
                    : "");
        } else {
            ui.hostCpu.textContent = "采样中";
            ui.hostLoad.textContent = `${system.cpu && system.cpu.cores || 1} 核`;
        }
        if (system.mem && system.mem.valid && system.mem.total_kb > 0) {
            const used = system.mem.total_kb - system.mem.available_kb;
            ui.hostMemory.textContent = formatBytes(used * 1024);
            ui.hostMemoryDetail.textContent =
                `总计 ${formatBytes(system.mem.total_kb * 1024)} · ` +
                `${(used / system.mem.total_kb * 100).toFixed(0)}%`;
        } else {
            ui.hostMemory.textContent = "--";
            ui.hostMemoryDetail.textContent = "不可用";
        }
        ui.hostProcess.textContent = system.process && system.process.valid
            ? formatBytes(system.process.rss_kb * 1024)
            : "--";
        if (system.disk && system.disk.valid && system.disk.total_bytes > 0) {
            const used = system.disk.total_bytes - system.disk.available_bytes;
            ui.hostDisk.textContent = formatBytes(used);
            ui.hostDiskDetail.textContent =
                `总计 ${formatBytes(system.disk.total_bytes)} · ` +
                `${(used / system.disk.total_bytes * 100).toFixed(0)}%`;
        } else {
            ui.hostDisk.textContent = "--";
            ui.hostDiskDetail.textContent = "不可用";
        }
    }

    function renderDevices() {
        if (!state.devices.length) {
            ui.devices.innerHTML =
                '<div class="empty">尚未收到设备心跳。开发板连接后会自动出现在这里。</div>';
            return;
        }
        const media = new Map(state.media.map((item) => [item.device_id, item]));
        ui.devices.innerHTML = state.devices.map((device) => {
            const current = media.get(device.device_id);
            const selected = device.device_id === state.device ? " selected" : "";
            const online = device.online ? "online" : "offline";
            return `<button class="device-card${selected}" type="button" data-device="${esc(device.device_id)}">
                <div class="device-title">
                    <strong>${esc(device.device_id)}</strong>
                    <span class="chip ${online}">${device.online ? "在线" : "离线"}</span>
                </div>
                <dl class="device-details">
                    <div><dt>IPv6</dt><dd title="${esc(device.ipv6 || "--")}">${esc(device.ipv6 || "--")}</dd></div>
                    <div><dt>远端录像</dt><dd>${current && current.recording ? "写入中" : "未录像"}${current ? ` · ${formatBytes(current.bytes)}` : ""}</dd></div>
                    <div><dt>固件</dt><dd>${esc(device.firmware || "--")}</dd></div>
                    <div><dt>心跳</dt><dd>${formatAge(device.age_seconds)}</dd></div>
                </dl>
            </button>`;
        }).join("");
        ui.devices.querySelectorAll("[data-device]").forEach((card) => {
            card.addEventListener("click", () => selectDevice(card.dataset.device || ""));
        });
    }

    function renderSettings(body, aiBody) {
        const settings = body && body.settings || {};
        const ai = aiBody && aiBody.ai || {};
        state.segmentSeconds = Number(settings.segment_seconds || 600);
        if (!state.settingsDirty) {
            ui.aiEnabled.checked = !!settings.ai_enabled;
            ui.aiInterval.value = settings.ai_interval_ms || 1000;
            ui.aiThreshold.value = Number(settings.ai_threshold || 0.3).toFixed(2);
            ui.aiMinPersonArea.value = (
                Number(settings.ai_min_person_area_ratio ?? 0.02) * 100
            ).toFixed(1);
            ui.aiCooldown.value = settings.ai_min_snapshot_seconds || 10;
            ui.aiSnapshotMaxCount.value = settings.ai_snapshot_max_count || 500;
            ui.aiSnapshotQuality.value = settings.ai_snapshot_quality || 95;
            ui.recordEnabled.checked = settings.record_enabled !== false;
            ui.segmentSeconds.value = state.segmentSeconds;
            ui.retainDays.value = settings.retain_days || 7;
            ui.maxGb.value = Math.max(
                1,
                Math.round(Number(settings.max_bytes || 0) / (1024 ** 3)),
            );
            setSettingsStatus("已同步", "active");
        }
        if (!ai.enabled) {
            ui.aiRuntimeStatus.textContent = "AI 已关闭";
        } else if (ai.available && ai.running) {
            ui.aiRuntimeStatus.textContent =
                `运行中 · 推理 ${ai.inference_count || 0} 次 · ` +
                `检测 ${ai.detection_count || 0} 次 · ` +
                `${Number(ai.last_inference_ms || 0).toFixed(0)} ms/次`;
        } else {
            ui.aiRuntimeStatus.textContent = ai.last_error || "AI 运行时不可用";
        }
    }

    function setSettingsStatus(text, stateName = "") {
        ui.settingsStatus.textContent = text;
        ui.settingsStatus.className = `chip ${stateName}`.trim();
    }

    async function saveSettings(event) {
        event.preventDefault();
        if (!ui.settingsForm.reportValidity()) return;
        const payload = {
            ai_enabled: ui.aiEnabled.checked,
            ai_interval_ms: Number(ui.aiInterval.value),
            ai_threshold: Number(ui.aiThreshold.value),
            ai_min_person_area_ratio: Number(ui.aiMinPersonArea.value) / 100,
            ai_min_snapshot_seconds: Number(ui.aiCooldown.value),
            ai_snapshot_max_count: Number(ui.aiSnapshotMaxCount.value),
            ai_snapshot_quality: Number(ui.aiSnapshotQuality.value),
            record_enabled: ui.recordEnabled.checked,
            segment_seconds: Number(ui.segmentSeconds.value),
            retain_days: Number(ui.retainDays.value),
            max_bytes: Number(ui.maxGb.value) * 1024 ** 3,
        };
        ui.saveSettings.disabled = true;
        setSettingsStatus("保存中");
        try {
            await api("/api/v1/settings", {
                method: "PUT",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(payload),
            });
            state.settingsDirty = false;
            setSettingsStatus("已保存", "active");
            showToast("camera-hub 设置已保存");
            await refreshAll(true);
        } catch (error) {
            setSettingsStatus("保存失败", "offline");
            handleError(error);
        } finally {
            ui.saveSettings.disabled = false;
        }
    }

    async function loadVoice(silent = false) {
        if (state.voiceBusy) return;
        state.voiceBusy = true;
        try {
            const body = await api("/api/v1/voice");
            renderVoice(body);
        } catch (error) {
            if (!silent) handleError(error);
        } finally {
            state.voiceBusy = false;
        }
    }

    function renderVoice(body) {
        const config = body && body.config || {};
        const status = body && body.status || {};
        const stale = !status.updated_epoch ||
            Date.now() / 1000 - Number(status.updated_epoch) > 15;
        const online = !!status.available && !stale;
        ui.voiceStatus.textContent = stale
            ? "进程离线"
            : status.running
                ? "正在监听"
                : status.state === "disabled"
                    ? "已停用"
                    : "等待音频";
        ui.voiceStatus.className = `chip ${online ? "active" : "offline"}`;
        ui.voiceState.textContent = status.state || "--";
        ui.voiceDetected.textContent = String(status.detected_count || 0);
        const rms = Number(status.audio_rms || 0);
        ui.voiceAudioLevel.textContent = rms > 0
            ? `${Math.max(-96, 20 * Math.log10(rms)).toFixed(1)} dBFS`
            : "--";
        ui.voiceLastKeyword.textContent = status.last_keyword || "--";
        ui.voiceLastError.textContent =
            status.last_error || (online ? "运行正常" : "等待 worker 状态");
        ui.voiceLastError.classList.toggle("error", !!status.last_error);

        if (!state.voiceDirty) {
            state.voiceConfig = structuredClone(config);
            ui.voiceEnabled.checked = !!config.enabled;
            ui.voiceCaptureDevice.value = config.capture_device || "hw:0,0";
            ui.voicePlaybackDevice.value = config.playback_device || "plughw:0,0";
            const playbackVolume = Number(config.playback_volume ?? 60);
            ui.voicePlaybackVolume.value = playbackVolume;
            ui.voicePlaybackVolumeValue.textContent = `${playbackVolume}%`;
            ui.voiceRequestTimeout.value = config.request_timeout_ms || 3000;
            ui.voiceGlobalCooldown.value = config.global_cooldown_ms || 2000;
            ui.voiceFailureReply.value = config.failure_reply || "操作失败，请稍后再试";
            renderVoiceCommands(Array.isArray(config.commands) ? config.commands : []);
        }
        renderVoiceEvents(Array.isArray(body.events) ? body.events : []);
    }

    function renderVoiceCommands(commands) {
        if (!commands.length) {
            ui.voiceCommandList.innerHTML = '<div class="empty">暂无语音命令</div>';
            return;
        }
        ui.voiceCommandList.innerHTML = commands.map((command, index) => `
            <article class="voice-command" data-command-index="${index}">
                <header>
                    <label class="toggle">
                        <input type="checkbox" data-voice-field="enabled"
                               ${command.enabled ? "checked" : ""}>
                        <span>${esc(command.phrase || "未命名命令")}</span>
                    </label>
                    <div class="voice-command-actions">
                        <button class="button ghost" type="button"
                                data-voice-action="reply">测试回复</button>
                        <button class="button ghost" type="button"
                                data-voice-action="request">测试接口</button>
                        <button class="button danger" type="button"
                                data-voice-action="delete">删除</button>
                    </div>
                </header>
                <div class="voice-command-grid">
                    <label><span>命令短语</span>
                        <input data-voice-field="phrase" type="text" maxlength="24"
                               value="${esc(command.phrase)}" required></label>
                    <label><span>成功回复</span>
                        <input data-voice-field="reply" type="text" maxlength="120"
                               value="${esc(command.reply)}" required></label>
                    <label><span>请求方式</span>
                        <select data-voice-field="method">
                            <option value="GET" ${command.method === "GET" ? "selected" : ""}>GET</option>
                            <option value="POST" ${command.method === "POST" ? "selected" : ""}>POST</option>
                        </select></label>
                    <label class="voice-url"><span>动作 URL</span>
                        <input data-voice-field="url" type="url" maxlength="2048"
                               value="${esc(command.url)}" placeholder="http://设备地址/action"></label>
                    <label class="voice-body"><span>POST JSON</span>
                        <input data-voice-field="body" type="text" maxlength="8192"
                               value="${esc(command.body)}" placeholder='{"enabled":true}'></label>
                    <label><span>Boost</span>
                        <input data-voice-field="boosting_score" type="number"
                               min="0" max="10" step="0.1"
                               value="${Number(command.boosting_score ?? 1.5).toFixed(1)}" required></label>
                    <label><span>触发阈值</span>
                        <input data-voice-field="trigger_threshold" type="number"
                               min="0.05" max="0.95" step="0.05"
                               value="${Number(command.trigger_threshold ?? 0.45).toFixed(2)}" required></label>
                    <label><span>冷却（毫秒）</span>
                        <input data-voice-field="cooldown_ms" type="number"
                               min="500" max="60000" step="100"
                               value="${Number(command.cooldown_ms || 2000)}" required></label>
                </div>
                <input data-voice-field="id" type="hidden" value="${esc(command.id)}">
            </article>`).join("");
    }

    function renderVoiceEvents(events) {
        if (!events.length) {
            ui.voiceEventList.innerHTML =
                '<tr><td colspan="5">暂无触发记录</td></tr>';
            return;
        }
        ui.voiceEventList.innerHTML = events.map((event) => `
            <tr>
                <td>${formatTimestamp(event.epoch)}</td>
                <td>${esc(event.phrase || event.command_id)}</td>
                <td>${event.source === "test" ? "测试" : "语音"}</td>
                <td class="${event.success ? "voice-success" : "voice-failure"}">
                    ${esc(event.message || (event.success ? "成功" : "失败"))}
                </td>
                <td>${Number(event.elapsed_ms || 0)} ms</td>
            </tr>`).join("");
    }

    function collectVoiceConfig() {
        const config = structuredClone(state.voiceConfig || {});
        config.enabled = ui.voiceEnabled.checked;
        config.capture_device = ui.voiceCaptureDevice.value.trim();
        config.playback_device = ui.voicePlaybackDevice.value.trim();
        config.playback_volume = Number(ui.voicePlaybackVolume.value);
        config.capture_rate = Number(config.capture_rate || 48000);
        config.request_timeout_ms = Number(ui.voiceRequestTimeout.value);
        config.global_cooldown_ms = Number(ui.voiceGlobalCooldown.value);
        config.failure_reply = ui.voiceFailureReply.value.trim();
        config.commands = Array.from(
            ui.voiceCommandList.querySelectorAll(".voice-command"),
        ).map((row) => {
            const field = (name) => row.querySelector(`[data-voice-field="${name}"]`);
            return {
                id: field("id").value,
                enabled: field("enabled").checked,
                phrase: field("phrase").value.trim(),
                reply: field("reply").value.trim(),
                method: field("method").value,
                url: field("url").value.trim(),
                body: field("body").value.trim(),
                boosting_score: Number(field("boosting_score").value),
                trigger_threshold: Number(field("trigger_threshold").value),
                cooldown_ms: Number(field("cooldown_ms").value),
            };
        });
        return config;
    }

    async function saveVoice(event) {
        event.preventDefault();
        if (!ui.voiceForm.reportValidity()) return;
        ui.saveVoice.disabled = true;
        try {
            const body = await api("/api/v1/voice", {
                method: "PUT",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(collectVoiceConfig()),
            });
            state.voiceConfig = structuredClone(body.config);
            state.voiceDirty = false;
            showToast("语音控制配置已保存");
            await loadVoice(true);
        } catch (error) {
            handleError(error);
        } finally {
            ui.saveVoice.disabled = false;
        }
    }

    function addVoiceCommand() {
        const commands = collectVoiceConfig().commands || [];
        commands.push({
            id: `command-${Date.now().toString(36)}`,
            enabled: false,
            phrase: "小雨",
            reply: "好的",
            method: "GET",
            url: "",
            body: "",
            boosting_score: 1.5,
            trigger_threshold: 0.45,
            cooldown_ms: 2000,
        });
        state.voiceDirty = true;
        renderVoiceCommands(commands);
        ui.voiceCommandList.lastElementChild?.querySelector(
            '[data-voice-field="phrase"]',
        )?.focus();
    }

    async function testVoiceCommand(commandId, callUrl) {
        if (state.voiceDirty) {
            showToast("请先保存语音配置", true);
            return;
        }
        try {
            await api("/api/v1/voice/test", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({
                    command_id: commandId,
                    call_url: callUrl,
                    speak_reply: !callUrl,
                }),
            });
            showToast(callUrl ? "接口测试已提交" : "回复测试已提交");
            window.setTimeout(() => loadVoice(true), 1200);
        } catch (error) {
            handleError(error);
        }
    }

    async function loadQq(silent = false) {
        if (state.qqBusy) return;
        state.qqBusy = true;
        try {
            const body = await api("/api/v1/qq");
            renderQq(body && body.qq || {});
        } catch (error) {
            if (!silent) handleError(error);
        } finally {
            state.qqBusy = false;
        }
    }

    function renderQq(body) {
        const config = body.config || {};
        const status = body.status || {};
        const online = !!status.online;
        ui.qqStatus.textContent = online
            ? "在线"
            : status.state === "connecting"
                ? "连接中"
                : status.state === "retrying"
                    ? "正在重连"
                    : status.state === "incomplete"
                        ? "配置不完整"
                        : "离线";
        ui.qqStatus.className = `chip ${online ? "active" : "offline"}`;
        ui.qqState.textContent = status.detail || status.state || "--";
        ui.qqBotName.textContent = status.bot_name || "--";
        ui.qqConnectedAt.textContent = formatTimestamp(status.connected_epoch);
        ui.qqSentCount.textContent = String(status.sent_count || 0);
        ui.qqLastError.textContent =
            status.last_error || (online ? "运行正常" : status.detail || "等待连接");
        ui.qqLastError.classList.toggle("error", !!status.last_error);
        ui.qqPushEndpoint.value =
            `${location.origin}${body.push_endpoint || "/api/v1/integrations/qq/notify"}`;
        ui.qqPushTokenState.textContent = config.push_token_configured
            ? "Token 已配置" : "尚未生成 Token";
        ui.qqPushTokenState.className =
            `chip ${config.push_token_configured ? "active" : ""}`.trim();

        if (!state.qqDirty) {
            state.qqConfig = structuredClone(config);
            ui.qqEnabled.checked = !!config.enabled;
            ui.qqAppId.value = config.app_id || "";
            ui.qqAppSecret.value = "";
            ui.qqSecretState.textContent = config.secret_configured
                ? "AppSecret 已配置，留空不会修改"
                : "尚未配置 AppSecret";
            ui.clearQqSecret.disabled = !config.secret_configured;
            renderQqGroups(Array.isArray(config.groups) ? config.groups : [], config.default_group);
        }
        updateQqCredentialRequirements();
    }

    function renderQqGroups(groups, defaultGroup) {
        ui.qqDefaultGroup.replaceChildren();
        ui.qqTestTarget.replaceChildren();
        ui.qqTestTarget.add(new Option("默认群", "default"));
        if (!groups.length) {
            ui.qqDefaultGroup.add(new Option("尚未发现群", ""));
            ui.qqDefaultGroup.disabled = true;
            ui.qqTestTarget.disabled = true;
            ui.qqGroupList.innerHTML =
                '<div class="empty">机器人加入群后会自动登记目标群</div>';
            return;
        }
        ui.qqDefaultGroup.add(new Option("请选择默认群", ""));
        for (const group of groups) {
            ui.qqDefaultGroup.add(new Option(group.name || group.openid, group.openid));
            ui.qqTestTarget.add(new Option(group.name || group.openid, group.openid));
        }
        ui.qqDefaultGroup.value = defaultGroup || "";
        ui.qqDefaultGroup.disabled = false;
        ui.qqTestTarget.disabled = false;
        ui.qqGroupList.innerHTML = groups.map((group) => `
            <label class="qq-group-row" data-qq-group="${esc(group.openid)}">
                <span>
                    <strong>${esc(group.name || "未命名群")}</strong>
                    <code title="${esc(group.openid)}">${esc(group.openid)}</code>
                </span>
                <input type="text" maxlength="32" value="${esc(group.name || "")}"
                       data-qq-group-alias="${esc(group.openid)}"
                       aria-label="群别名">
            </label>`).join("");
    }

    function updateQqCredentialRequirements() {
        ui.qqAppId.required = ui.qqEnabled.checked;
        ui.qqAppSecret.required =
            ui.qqEnabled.checked && !(state.qqConfig && state.qqConfig.secret_configured);
    }

    function collectQqConfig(clearSecret = false) {
        const aliases = {};
        ui.qqGroupList.querySelectorAll("[data-qq-group-alias]").forEach((input) => {
            aliases[input.dataset.qqGroupAlias] = input.value.trim();
        });
        return {
            enabled: clearSecret ? false : ui.qqEnabled.checked,
            app_id: ui.qqAppId.value.trim(),
            app_secret: clearSecret ? "" : ui.qqAppSecret.value.trim(),
            clear_secret: clearSecret,
            default_group: ui.qqDefaultGroup.value,
            group_aliases: aliases,
        };
    }

    async function saveQqConfig(event, clearSecret = false) {
        if (event) event.preventDefault();
        if (!clearSecret && !ui.qqForm.reportValidity()) return;
        ui.saveQq.disabled = true;
        ui.clearQqSecret.disabled = true;
        try {
            await api("/api/v1/qq", {
                method: "PUT",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(collectQqConfig(clearSecret)),
            });
            state.qqDirty = false;
            ui.qqAppSecret.value = "";
            showToast(clearSecret ? "QQ AppSecret 已清除" : "QQ 机器人配置已保存");
            await loadQq(true);
        } catch (error) {
            handleError(error);
        } finally {
            ui.saveQq.disabled = false;
            ui.clearQqSecret.disabled = false;
        }
    }

    async function rotateQqPushToken() {
        const configured = !!(state.qqConfig && state.qqConfig.push_token_configured);
        if (configured && !window.confirm("重新生成后，旧 Push Token 会立即失效。继续？")) {
            return;
        }
        ui.rotateQqPushToken.disabled = true;
        try {
            const body = await api("/api/v1/qq/push-token", { method: "POST" });
            ui.qqPushToken.value = body.token || "";
            ui.copyQqPushToken.disabled = !ui.qqPushToken.value;
            if (state.qqConfig) state.qqConfig.push_token_configured = true;
            ui.qqPushTokenState.textContent = "新 Token 仅显示一次";
            ui.qqPushTokenState.className = "chip active";
            showToast("新 Push Token 已生成");
        } catch (error) {
            handleError(error);
        } finally {
            ui.rotateQqPushToken.disabled = false;
        }
    }

    async function copyQqPushToken() {
        if (!ui.qqPushToken.value) return;
        try {
            await navigator.clipboard.writeText(ui.qqPushToken.value);
        } catch (_) {
            ui.qqPushToken.select();
            document.execCommand("copy");
        }
        showToast("Push Token 已复制");
    }

    async function sendQqTest(event) {
        event.preventDefault();
        if (!ui.qqTestForm.reportValidity()) return;
        if (state.qqDirty) {
            showToast("请先保存 QQ 机器人配置", true);
            return;
        }
        ui.sendQqTest.disabled = true;
        try {
            const body = await api("/api/v1/qq/test", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({
                    target: ui.qqTestTarget.value,
                    content: ui.qqTestMessage.value.trim(),
                }),
            });
            showToast(
                `已向 ${body.delivery?.target || "目标群"}发送 ` +
                `${body.delivery?.messages || 1} 条消息`,
            );
            await loadQq(true);
        } catch (error) {
            handleError(error);
        } finally {
            ui.sendQqTest.disabled = false;
        }
    }

    function renderDeviceSelect() {
        for (const select of [
            ui.deviceSelect,
            ui.liveDeviceSelect,
            ui.photoDeviceSelect,
        ]) {
            select.replaceChildren();
            if (!state.devices.length) {
                select.add(new Option("暂无设备", ""));
                select.disabled = true;
                continue;
            }
            state.devices.forEach((device) => {
                select.add(
                    new Option(
                        `${device.device_id} · ${device.online ? "在线" : "离线"}`,
                        device.device_id,
                    ),
                );
            });
            select.value = state.device;
            select.disabled = false;
        }
    }

    async function selectDevice(deviceId) {
        if (!deviceId || deviceId === state.device) return;
        await stopHubLive();
        stopRecordPlayback();
        state.device = deviceId;
        state.date = "";
        state.photoSelection.clear();
        renderDevices();
        renderDeviceSelect();
        await loadDays();
        if (state.view === "photos") await loadPhotos();
    }

    async function loadDays() {
        ui.dateSelect.replaceChildren();
        if (!state.device) {
            state.days = [];
            state.date = "";
            ui.dateSelect.add(new Option("暂无录像", ""));
            ui.dateSelect.disabled = true;
            renderRecords([]);
            return;
        }
        try {
            const body = await api(
                `/api/v1/devices/${encodeURIComponent(state.device)}/records/days`,
            );
            state.days = Array.isArray(body.days) ? body.days : [];
            if (!state.days.some((item) => item.date === state.date)) {
                state.date = state.days[0] ? state.days[0].date : "";
            }
            if (!state.days.length) {
                ui.dateSelect.add(new Option("暂无录像", ""));
                ui.dateSelect.disabled = true;
                renderRecords([]);
                return;
            }
            state.days.forEach((day) => {
                ui.dateSelect.add(
                    new Option(`${formatDate(day.date)} · ${day.files} 个文件`, day.date),
                );
            });
            ui.dateSelect.value = state.date;
            ui.dateSelect.disabled = false;
            await loadRecords();
        } catch (error) {
            handleError(error);
        }
    }

    async function loadRecords() {
        if (!state.device || !state.date) {
            renderRecords([]);
            return;
        }
        ui.summary.textContent = "正在读取远端录像目录…";
        try {
            const body = await api(
                `/api/v1/devices/${encodeURIComponent(state.device)}` +
                `/records/${encodeURIComponent(state.date)}`,
            );
            renderRecords(Array.isArray(body.records) ? body.records : []);
        } catch (error) {
            handleError(error);
        }
    }

    function renderRecords(records) {
        state.records = buildTimelineRecords(records);
        const day = state.days.find((item) => item.date === state.date);
        if (!state.device) {
            ui.summary.textContent = "请选择设备查看远端录像。";
        } else if (!state.date) {
            ui.summary.textContent = `${state.device} 暂无远端录像。`;
        } else {
            ui.summary.textContent =
                `${state.device} · ${formatDate(state.date)} · ${records.length} 个文件 · ` +
                formatBytes(day ? day.bytes : 0);
        }
        if (!records.length) {
            ui.records.innerHTML =
                '<tr><td colspan="6" class="empty-cell">该日期暂无录像文件</td></tr>';
            stopRecordPlayback();
            drawTimeline();
            return;
        }
        ui.records.innerHTML = records.map((record) => {
            const url = recordUrl(record.name);
            return `<tr>
            <td>${esc(record.time || "--")}</td>
            <td class="file-name" title="${esc(record.name)}">${esc(record.name)}</td>
            <td>${formatBytes(record.size)}</td>
            <td><span class="chip ${record.active ? "active" : ""}">${record.active ? "写入中" : "已完成"}</span></td>
            <td>${formatTimestamp(record.modified_epoch)}</td>
            <td><button class="table-action" type="button" data-play="${esc(record.name)}">播放</button>
                <a class="table-action" href="${url}" download="${esc(record.name)}">下载</a></td>
        </tr>`;
        }).join("");
        ui.records.querySelectorAll("[data-play]").forEach((button) => {
            button.addEventListener("click", () => playRecord(button.dataset.play || ""));
        });
        drawTimeline();
    }

    function recordStartSeconds(record) {
        const match = /^(\d{2}):(\d{2}):(\d{2})$/.exec(record.time || "");
        if (!match) return 0;
        return Number(match[1]) * 3600 + Number(match[2]) * 60 + Number(match[3]);
    }

    function buildTimelineRecords(records) {
        const ordered = records
            .map((record) => ({ ...record, startSeconds: recordStartSeconds(record) }))
            .sort((left, right) => left.startSeconds - right.startSeconds);
        ordered.forEach((record, index) => {
            const next = ordered[index + 1];
            const gap = next ? next.startSeconds - record.startSeconds : state.segmentSeconds;
            record.duration = Math.max(1, Math.min(state.segmentSeconds, gap));
        });
        return ordered;
    }

    function photoUrl(name) {
        return `/photos/${encodeURIComponent(state.device)}/${encodeURIComponent(name)}`;
    }

    async function loadPhotos() {
        if (!state.device) {
            state.photos = [];
            renderPhotos();
            return;
        }
        ui.refreshPhotos.disabled = true;
        ui.photoSummary.textContent = "正在读取节点 AI 抓拍…";
        try {
            const body = await api(
                `/api/v1/devices/${encodeURIComponent(state.device)}/photos`,
            );
            state.photos = Array.isArray(body.photos) ? body.photos : [];
            const available = new Set(state.photos.map((photo) => photo.name));
            for (const name of state.photoSelection) {
                if (!available.has(name)) state.photoSelection.delete(name);
            }
            renderPhotos();
        } catch (error) {
            state.photos = [];
            renderPhotos();
            handleError(error);
        } finally {
            ui.refreshPhotos.disabled = false;
        }
    }

    function renderPhotos() {
        ui.photoEmpty.hidden = state.photos.length > 0;
        ui.photoGrid.innerHTML = state.photos.map((photo) => {
            const url = photoUrl(photo.name);
            const selected = state.photoSelection.has(photo.name);
            return `<article class="photo-card${selected ? " selected" : ""}"
                            data-photo-card="${esc(photo.name)}">
                <button class="photo-open" type="button" data-open-photo="${esc(photo.name)}">
                    <img loading="lazy" src="${url}" alt="${esc(photo.name)}">
                    <span>${esc(formatPhotoName(photo.name))}</span>
                    <small>${formatBytes(photo.size)} · ${formatTimestamp(photo.modified_epoch)}</small>
                </button>
                <input class="photo-select" type="checkbox"
                       data-select-photo="${esc(photo.name)}"
                       aria-label="选择 ${esc(photo.name)}"${selected ? " checked" : ""}>
            </article>`;
        }).join("");
        ui.photoGrid.querySelectorAll("[data-open-photo]").forEach((button) => {
            button.addEventListener("click", () => openPhoto(button.dataset.openPhoto || ""));
        });
        ui.photoGrid.querySelectorAll("[data-select-photo]").forEach((checkbox) => {
            checkbox.addEventListener("change", () => {
                const name = checkbox.dataset.selectPhoto || "";
                if (checkbox.checked) state.photoSelection.add(name);
                else state.photoSelection.delete(name);
                checkbox.closest("[data-photo-card]")?.classList.toggle(
                    "selected",
                    checkbox.checked,
                );
                updatePhotoSelectionControls();
            });
        });
        updatePhotoSelectionControls();
    }

    function updatePhotoSelectionControls() {
        const selected = state.photoSelection.size;
        ui.photoSummary.textContent = state.device
            ? `${state.device} · camera-hub 节点保留 ${state.photos.length} 张 AI 抓拍` +
                (selected ? ` · 已选 ${selected} 张` : "")
            : "请选择设备查看 AI 抓拍。";
        ui.selectAllPhotos.disabled =
            !state.device || !state.photos.length || selected === state.photos.length;
        ui.clearPhotoSelection.disabled = selected === 0;
        ui.deleteSelectedPhotos.disabled = selected === 0;
        ui.clearPhotos.disabled = !state.device || state.photos.length === 0;
    }

    function selectAllPhotos() {
        for (const photo of state.photos) state.photoSelection.add(photo.name);
        renderPhotos();
    }

    function clearPhotoSelection() {
        state.photoSelection.clear();
        renderPhotos();
    }

    function formatPhotoName(name) {
        const match =
            /^(\d{4})(\d{2})(\d{2})_(\d{2})(\d{2})(\d{2})_(\d{3})\.jpg$/.exec(name);
        return match
            ? `${match[1]}-${match[2]}-${match[3]} ${match[4]}:${match[5]}:${match[6]}`
            : name;
    }

    function openPhoto(name) {
        if (!name) return;
        const url = photoUrl(name);
        ui.photoPreview.src = url;
        ui.photoPreview.alt = name;
        ui.downloadPhoto.href = url;
        ui.downloadPhoto.download = name;
        state.currentPhoto = name;
        ui.deletePhoto.disabled = false;
        ui.photoLightbox.hidden = false;
    }

    function closePhoto() {
        ui.photoLightbox.hidden = true;
        ui.photoPreview.removeAttribute("src");
        state.currentPhoto = "";
    }

    async function deleteCurrentPhoto() {
        const name = state.currentPhoto;
        if (!state.device || !name || ui.deletePhoto.disabled) return;
        if (!window.confirm(
            "删除当前 Camera-hub 照片？开发板相册中的同步副本不会删除。",
        )) {
            return;
        }
        ui.deletePhoto.disabled = true;
        try {
            await api(
                `/api/v1/devices/${encodeURIComponent(state.device)}/photos/` +
                encodeURIComponent(name),
                { method: "DELETE" },
            );
            closePhoto();
            state.photoSelection.delete(name);
            state.photos = state.photos.filter((photo) => photo.name !== name);
            renderPhotos();
            showToast("Camera-hub 照片已删除");
        } catch (error) {
            ui.deletePhoto.disabled = false;
            handleError(error);
        }
    }

    async function deleteSelectedPhotos() {
        const names = Array.from(state.photoSelection);
        if (!state.device || !names.length || ui.deleteSelectedPhotos.disabled) return;
        if (!window.confirm(
            `删除选中的 ${names.length} 张 Camera-hub 照片？` +
            "开发板相册中的同步副本不会删除。",
        )) {
            return;
        }
        ui.deleteSelectedPhotos.disabled = true;
        try {
            const body = await api(
                `/api/v1/devices/${encodeURIComponent(state.device)}/photos/delete`,
                {
                    method: "POST",
                    headers: { "Content-Type": "application/json" },
                    body: JSON.stringify({ names }),
                },
            );
            const deleted = Number(body.deleted?.deleted || 0);
            const missing = Number(body.deleted?.missing || 0);
            const errors = Array.isArray(body.deleted?.errors) ? body.deleted.errors : [];
            if (errors.length) {
                throw new Error(`${errors.length} 张照片删除失败`);
            }
            closePhoto();
            state.photoSelection.clear();
            await loadPhotos();
            showToast(
                `已删除 ${deleted} 张 Camera-hub 照片` +
                (missing ? `，${missing} 张已不存在` : ""),
            );
        } catch (error) {
            updatePhotoSelectionControls();
            handleError(error);
        }
    }

    async function clearCurrentPhotos() {
        if (!state.device || !state.photos.length || ui.clearPhotos.disabled) return;
        const count = state.photos.length;
        if (!window.confirm(
            `清空 ${state.device} 在 Camera-hub 保存的 ${count} 张照片？` +
            "开发板相册中的同步副本不会删除。",
        )) {
            return;
        }
        ui.clearPhotos.disabled = true;
        try {
            const body = await api(
                `/api/v1/devices/${encodeURIComponent(state.device)}/photos`,
                { method: "DELETE" },
            );
            closePhoto();
            state.photoSelection.clear();
            state.photos = [];
            renderPhotos();
            showToast(`已删除 ${Number(body.deleted?.files || 0)} 张 Camera-hub 照片`);
        } catch (error) {
            ui.clearPhotos.disabled = false;
            handleError(error);
        }
    }

    function recordUrl(name) {
        return `/records/${encodeURIComponent(state.device)}/` +
            `${encodeURIComponent(state.date)}/${encodeURIComponent(name)}`;
    }

    function formatDayTime(seconds) {
        const value = Math.max(0, Math.min(86399, Math.floor(Number(seconds) || 0)));
        const hours = Math.floor(value / 3600);
        const minutes = Math.floor((value % 3600) / 60);
        const secs = value % 60;
        return [hours, minutes, secs].map((part) => String(part).padStart(2, "0")).join(":");
    }

    function timelineCursorSeconds() {
        if (state.timelinePreview != null) return state.timelinePreview;
        const record = state.records.find((item) => item.name === state.currentRecord);
        return record ? record.startSeconds + Number(ui.player.currentTime || 0) : null;
    }

    function resizeTimeline() {
        const ratio = window.devicePixelRatio || 1;
        const width = Math.max(
            280,
            Math.floor(ui.timeline.getBoundingClientRect().width || 800),
        );
        const height = window.innerWidth <= 620 ? 64 : 72;
        ui.timeline.width = Math.floor(width * ratio);
        ui.timeline.height = Math.floor(height * ratio);
        ui.timeline.style.height = `${height}px`;
        drawTimeline();
    }

    function drawTimeline() {
        const context = ui.timeline.getContext("2d");
        const width = ui.timeline.width;
        const height = ui.timeline.height;
        const ratio = window.devicePixelRatio || 1;
        if (!width || !height) return;
        context.clearRect(0, 0, width, height);
        context.fillStyle = "#080b1a";
        context.fillRect(0, 0, width, height);
        context.font = `${10 * ratio}px ui-monospace, monospace`;
        context.textBaseline = "top";
        for (let hour = 0; hour <= 24; hour += 1) {
            const x = Math.round(width * hour / 24);
            const major = hour % 6 === 0;
            context.strokeStyle = major
                ? "rgba(162,173,255,.28)" : "rgba(162,173,255,.10)";
            context.lineWidth = ratio;
            context.beginPath();
            context.moveTo(x, major ? 0 : 18 * ratio);
            context.lineTo(x, height);
            context.stroke();
            if (major && hour < 24) {
                context.fillStyle = "#7f88af";
                context.fillText(`${String(hour).padStart(2, "0")}:00`, x + 4 * ratio, 5 * ratio);
            }
        }
        const top = 25 * ratio;
        const trackHeight = height - 34 * ratio;
        for (const record of state.records) {
            const start = width * record.startSeconds / 86400;
            const end = width * (record.startSeconds + record.duration) / 86400;
            context.fillStyle = record.name === state.currentRecord
                ? "#a2adff" : "#6175f5";
            context.fillRect(start, top, Math.max(2 * ratio, end - start), trackHeight);
        }
        const cursor = timelineCursorSeconds();
        if (cursor != null) {
            const x = width * Math.max(0, Math.min(86399, cursor)) / 86400;
            context.strokeStyle = state.timelinePreview != null ? "#fbbf24" : "#fb7185";
            context.lineWidth = 2 * ratio;
            context.beginPath();
            context.moveTo(x, 0);
            context.lineTo(x, height);
            context.stroke();
            ui.timelinePosition.textContent = formatDayTime(cursor);
            ui.timeline.setAttribute("aria-valuenow", String(Math.floor(cursor)));
            ui.timeline.setAttribute("aria-valuetext", formatDayTime(cursor));
        } else {
            ui.timelinePosition.textContent = "--:--:--";
            ui.timeline.setAttribute("aria-valuenow", "0");
            ui.timeline.removeAttribute("aria-valuetext");
        }
    }

    function timelineSecondsAt(clientX) {
        const bounds = ui.timeline.getBoundingClientRect();
        const ratio = Math.max(0, Math.min(1, (clientX - bounds.left) / bounds.width));
        return Math.floor(ratio * 86399);
    }

    function timelineRecordAt(seconds) {
        return state.records.find((record) =>
            seconds >= record.startSeconds &&
            seconds < record.startSeconds + record.duration
        ) || null;
    }

    function showTimelinePreview(seconds, clientX) {
        state.timelinePreview = seconds;
        const record = timelineRecordAt(seconds);
        ui.timelineTip.textContent =
            `${formatDayTime(seconds)} · ${record ? record.time : "无录像"}`;
        const bounds = ui.timeline.getBoundingClientRect();
        const left = Math.max(42, Math.min(bounds.width - 42, clientX - bounds.left));
        ui.timelineTip.style.left = `${left}px`;
        ui.timelineTip.hidden = false;
        drawTimeline();
    }

    function seekTimeline(seconds) {
        const record = timelineRecordAt(seconds);
        const target = record || state.records.find((item) => item.startSeconds >= seconds);
        state.timelinePreview = null;
        ui.timelineTip.hidden = true;
        if (!target) {
            drawTimeline();
            return;
        }
        const offset = record ? seconds - target.startSeconds : 0;
        playRecord(target.name, offset);
    }

    const PLAYBACK_MIME = 'video/mp4; codecs="avc1.4d001f, mp4a.40.2"';
    const StandardMediaSource = window.MediaSource || window.WebKitMediaSource || null;
    const ManagedMediaSource = window.ManagedMediaSource || null;
    const PlaybackMediaSource = StandardMediaSource || ManagedMediaSource;
    let playbackEpoch = 0;
    let playbackAttachment = null;
    let playbackSource = null;
    let playbackBuffer = null;
    let playbackAbort = null;
    let playbackIndex = null;
    let playbackUrl = "";
    let appendedFragments = null;
    let refillBusy = false;
    let playbackSeekListener = null;
    let playbackTimeListener = null;
    let desiredPlaybackRate = 1;
    let playbackReloadAt = 0;

    function setPlayerStatus(text) {
        ui.playerStatus.textContent = text;
    }

    function applyPlaybackRate() {
        try { ui.player.defaultPlaybackRate = desiredPlaybackRate; } catch (_) {}
        try { ui.player.playbackRate = desiredPlaybackRate; } catch (_) {}
        ui.player.muted = desiredPlaybackRate > 2;
    }

    function supportsMse() {
        return PlaybackMediaSource &&
            (typeof PlaybackMediaSource.isTypeSupported !== "function" ||
                PlaybackMediaSource.isTypeSupported(PLAYBACK_MIME));
    }

    function attachPlaybackSource() {
        const mediaSource = new PlaybackMediaSource();
        const objectUrl = URL.createObjectURL(mediaSource);
        let sourceElement = null;
        ui.player.disableRemotePlayback = true;
        if (ManagedMediaSource && PlaybackMediaSource === ManagedMediaSource) {
            sourceElement = document.createElement("source");
            sourceElement.dataset.mseSource = "1";
            sourceElement.type = "video/mp4";
            sourceElement.src = objectUrl;
            ui.player.appendChild(sourceElement);
            ui.player.load();
        } else {
            ui.player.src = objectUrl;
        }
        return { mediaSource, objectUrl, sourceElement };
    }

    function waitForSourceOpen(mediaSource) {
        if (mediaSource.readyState === "open") return Promise.resolve(true);
        return new Promise((resolve) => {
            let finished = false;
            const done = (opened) => {
                if (finished) return;
                finished = true;
                clearTimeout(timer);
                mediaSource.removeEventListener("sourceopen", onOpen);
                mediaSource.removeEventListener("webkitsourceopen", onOpen);
                resolve(opened);
            };
            const onOpen = () => done(true);
            const timer = setTimeout(() => done(false), 5000);
            mediaSource.addEventListener("sourceopen", onOpen, { once: true });
            mediaSource.addEventListener("webkitsourceopen", onOpen, { once: true });
        });
    }

    function waitForAppend(sourceBuffer, epoch) {
        return new Promise((resolve) => {
            const finish = (ok) => {
                sourceBuffer.removeEventListener("updateend", onEnd);
                sourceBuffer.removeEventListener("error", onError);
                resolve(ok && epoch === playbackEpoch);
            };
            const onEnd = () => finish(true);
            const onError = () => finish(false);
            sourceBuffer.addEventListener("updateend", onEnd, { once: true });
            sourceBuffer.addEventListener("error", onError, { once: true });
        });
    }

    async function appendPlaybackBytes(bytes, epoch) {
        if (epoch !== playbackEpoch || !playbackBuffer) return false;
        try {
            playbackBuffer.appendBuffer(bytes);
        } catch (error) {
            console.warn("MSE append failed", error);
            return false;
        }
        return waitForAppend(playbackBuffer, epoch);
    }

    async function fetchPlaybackIndex(url, signal) {
        try {
            const response = await fetch(`${url}.idx`, { signal, cache: "no-store" });
            if (!response.ok) return null;
            let text = (await response.text()).trim();
            if (!text.startsWith("{")) return null;
            let value;
            try {
                value = JSON.parse(text);
            } catch (_) {
                const lastBracket = text.lastIndexOf("]");
                if (lastBracket < 0) return null;
                try {
                    value = JSON.parse(`${text.slice(0, lastBracket + 1)}]}`);
                } catch (_) {
                    return null;
                }
            }
            return value && value.v === 1 && value.init > 0 &&
                Array.isArray(value.frags) ? value : null;
        } catch (_) {
            return null;
        }
    }

    function fragmentAt(seconds) {
        if (!playbackIndex || !playbackIndex.frags.length) return 0;
        const target = seconds * (playbackIndex.ts || 90000);
        let low = 0;
        let high = playbackIndex.frags.length - 1;
        let answer = 0;
        while (low <= high) {
            const middle = (low + high) >> 1;
            if (playbackIndex.frags[middle][0] <= target) {
                answer = middle;
                low = middle + 1;
            } else {
                high = middle - 1;
            }
        }
        return answer;
    }

    function bufferedContains(seconds) {
        const ranges = ui.player.buffered;
        for (let index = 0; ranges && index < ranges.length; index += 1) {
            if (seconds >= ranges.start(index) - 0.05 &&
                seconds < ranges.end(index) - 0.05) return true;
        }
        return false;
    }

    async function appendFragmentBatch(start, epoch, signal) {
        if (refillBusy || epoch !== playbackEpoch || !playbackIndex) return false;
        while (start < playbackIndex.frags.length && appendedFragments.has(start)) start += 1;
        if (start >= playbackIndex.frags.length) return false;
        refillBusy = true;
        try {
            const timeScale = playbackIndex.ts || 90000;
            const startedAt = playbackIndex.frags[start][0];
            let end = start;
            while (end + 1 < playbackIndex.frags.length &&
                !appendedFragments.has(end + 1) &&
                (playbackIndex.frags[end + 1][0] - startedAt) / timeScale < 10) {
                end += 1;
            }
            const first = playbackIndex.frags[start];
            const last = playbackIndex.frags[end];
            const response = await fetch(playbackUrl, {
                signal,
                cache: "no-store",
                headers: { Range: `bytes=${first[1]}-${last[1] + last[2] - 1}` },
            });
            if (epoch !== playbackEpoch || (!response.ok && response.status !== 206)) return false;
            const ok = await appendPlaybackBytes(
                new Uint8Array(await response.arrayBuffer()),
                epoch,
            );
            if (!ok) return false;
            for (let index = start; index <= end; index += 1) {
                appendedFragments.add(index);
            }
            return true;
        } finally {
            refillBusy = false;
        }
    }

    async function ensurePlaybackAt(seconds, epoch, forceSeek = false) {
        if (epoch !== playbackEpoch || !playbackIndex) return;
        if (bufferedContains(seconds)) return;
        const fragment = fragmentAt(seconds);
        const ok = await appendFragmentBatch(fragment, epoch, playbackAbort.signal);
        if (ok && forceSeek && epoch === playbackEpoch) {
            try {
                ui.player.currentTime = seconds;
                await ui.player.play();
            } catch (_) {}
        }
    }

    function fallbackNativePlayback(url, reason, name, seekOffset) {
        stopRecordPlayback(false, false);
        state.currentRecord = name;
        ui.playerBox.hidden = false;
        ui.player.src = url;
        ui.player.load();
        applyPlaybackRate();
        if (seekOffset > 0) {
            ui.player.addEventListener("loadedmetadata", () => {
                try { ui.player.currentTime = seekOffset; } catch (_) {}
            }, { once: true });
        }
        setPlayerStatus(`${reason}，已切换原生 MP4 播放`);
        ui.player.play().catch(() => {});
        drawTimeline();
    }

    async function playRecord(name, seekOffset = 0) {
        if (!name) return;
        stopRecordPlayback(false);
        const epoch = playbackEpoch;
        const url = recordUrl(name);
        const wantedTime = Math.max(0, Number(seekOffset) || 0);
        state.currentRecord = name;
        playbackUrl = url;
        ui.playerTitle.textContent = `${state.device} · ${name}`;
        ui.recordDownload.href = url;
        ui.recordDownload.download = name;
        ui.playerBox.hidden = false;
        setPlayerStatus("正在读取录像索引…");
        drawTimeline();
        ui.playerBox.scrollIntoView({ behavior: "smooth", block: "nearest" });

        if (!supportsMse()) {
            fallbackNativePlayback(url, "当前浏览器不支持 MSE", name, wantedTime);
            return;
        }
        playbackAbort = new AbortController();
        playbackIndex = await fetchPlaybackIndex(url, playbackAbort.signal);
        if (epoch !== playbackEpoch) return;
        if (!playbackIndex || !playbackIndex.frags.length) {
            fallbackNativePlayback(url, "录像索引不可用", name, wantedTime);
            return;
        }

        try {
            playbackAttachment = attachPlaybackSource();
            playbackSource = playbackAttachment.mediaSource;
            if (!await waitForSourceOpen(playbackSource) || epoch !== playbackEpoch) {
                fallbackNativePlayback(url, "媒体源打开失败", name, wantedTime);
                return;
            }
            URL.revokeObjectURL(playbackAttachment.objectUrl);
            playbackAttachment.objectUrl = "";
            playbackBuffer = playbackSource.addSourceBuffer(PLAYBACK_MIME);
            playbackBuffer.mode = "segments";
            const initResponse = await fetch(url, {
                signal: playbackAbort.signal,
                cache: "no-store",
                headers: { Range: `bytes=0-${playbackIndex.init - 1}` },
            });
            if (!initResponse.ok && initResponse.status !== 206) {
                throw new Error(`init HTTP ${initResponse.status}`);
            }
            const initOk = await appendPlaybackBytes(
                new Uint8Array(await initResponse.arrayBuffer()),
                epoch,
            );
            if (!initOk || epoch !== playbackEpoch) return;
            appendedFragments = new Set();
            const fragments = playbackIndex.frags;
            const timeScale = playbackIndex.ts || 90000;
            const last = fragments.length - 1;
            const lastDuration = last > 0
                ? (fragments[last][0] - fragments[last - 1][0]) / timeScale : 1;
            playbackSource.duration =
                fragments[last][0] / timeScale + Math.max(lastDuration, 0.5);
            const timelineRecord = state.records.find((record) => record.name === name);
            if (timelineRecord) timelineRecord.duration = playbackSource.duration;
            const initialFragment = fragmentAt(wantedTime);
            if (!await appendFragmentBatch(initialFragment, epoch, playbackAbort.signal)) {
                throw new Error("首批媒体加载失败");
            }
            const ranges = ui.player.buffered;
            if (ranges.length) {
                ui.player.currentTime = Math.max(wantedTime, ranges.start(0) + 0.001);
            }
            setPlayerStatus(
                `索引播放 · ${fragments.length} 个片段 · ` +
                `约 ${formatDuration(playbackSource.duration)}`,
            );
            applyPlaybackRate();
            ui.player.play().catch(() => {});

            playbackSeekListener = () => {
                ensurePlaybackAt(ui.player.currentTime || 0, epoch, true);
            };
            playbackTimeListener = () => {
                const current = ui.player.currentTime || 0;
                const rangesNow = ui.player.buffered;
                let remaining = 0;
                for (let index = 0; index < rangesNow.length; index += 1) {
                    if (current >= rangesNow.start(index) - 0.05 &&
                        current <= rangesNow.end(index) + 0.05) {
                        remaining = rangesNow.end(index) - current;
                        break;
                    }
                }
                if (remaining < 5) {
                    ensurePlaybackAt(current + 6, epoch);
                }
            };
            ui.player.addEventListener("seeking", playbackSeekListener);
            ui.player.addEventListener("timeupdate", playbackTimeListener);
        } catch (error) {
            if (epoch === playbackEpoch) {
                console.warn("indexed playback failed", error);
                fallbackNativePlayback(
                    url,
                    error.message || "索引播放失败",
                    name,
                    wantedTime,
                );
            }
        }
    }

    function stopRecordPlayback(hide = true, clearSelection = true) {
        playbackEpoch += 1;
        try { playbackAbort && playbackAbort.abort(); } catch (_) {}
        playbackAbort = null;
        if (playbackSeekListener) {
            ui.player.removeEventListener("seeking", playbackSeekListener);
            playbackSeekListener = null;
        }
        if (playbackTimeListener) {
            ui.player.removeEventListener("timeupdate", playbackTimeListener);
            playbackTimeListener = null;
        }
        try {
            if (playbackSource && playbackSource.readyState === "open") {
                playbackSource.endOfStream();
            }
        } catch (_) {}
        if (playbackAttachment) {
            if (playbackAttachment.objectUrl) {
                try { URL.revokeObjectURL(playbackAttachment.objectUrl); } catch (_) {}
            }
            if (playbackAttachment.sourceElement) {
                playbackAttachment.sourceElement.remove();
            }
        }
        ui.player.pause();
        playbackReloadAt = performance.now();
        ui.player.removeAttribute("src");
        ui.player.load();
        applyPlaybackRate();
        playbackAttachment = null;
        playbackSource = null;
        playbackBuffer = null;
        playbackIndex = null;
        appendedFragments = null;
        refillBusy = false;
        if (clearSelection) state.currentRecord = "";
        state.timelinePreview = null;
        ui.timelineTip.hidden = true;
        if (hide) ui.playerBox.hidden = true;
        drawTimeline();
    }

    let liveSocket = null;
    let liveAttachment = null;
    let liveSource = null;
    let liveBuffer = null;
    let liveQueue = [];
    let liveQueueBytes = 0;
    let livePeer = null;
    let liveMode = "";
    let liveStarted = false;
    let liveSession = 0;

    function setLiveStatus(text, online = false) {
        ui.liveStatus.textContent = text;
        ui.liveStatus.classList.toggle("online", online);
    }

    function setLiveButtons(activeMode = "") {
        const buttons = [
            [ui.startSmoothLive, "mse"],
            [ui.startFlvLive, "flv"],
            [ui.startLowLive, "webrtc"],
            [ui.startMoqLive, "moq"],
        ];
        for (const [button, mode] of buttons) {
            const active = mode === activeMode;
            button.disabled = active;
            button.dataset.active = String(active);
        }
        ui.stopLive.disabled = !activeMode;
    }

    function updateLiveClock() {
        const now = new Date();
        const parts = [
            now.getFullYear(),
            String(now.getMonth() + 1).padStart(2, "0"),
            String(now.getDate()).padStart(2, "0"),
        ];
        const time = [
            String(now.getHours()).padStart(2, "0"),
            String(now.getMinutes()).padStart(2, "0"),
            String(now.getSeconds()).padStart(2, "0"),
        ].join(":");
        ui.liveClock.dateTime = now.toISOString();
        ui.liveClock.textContent = parts.join("-") + " " + time + "." +
            String(now.getMilliseconds()).padStart(3, "0");
    }

    function attachLiveSource() {
        const mediaSource = new PlaybackMediaSource();
        const objectUrl = URL.createObjectURL(mediaSource);
        let sourceElement = null;
        ui.livePlayer.disableRemotePlayback = true;
        if (ManagedMediaSource && PlaybackMediaSource === ManagedMediaSource) {
            sourceElement = document.createElement("source");
            sourceElement.dataset.mseSource = "1";
            sourceElement.type = "video/mp4";
            sourceElement.src = objectUrl;
            ui.livePlayer.appendChild(sourceElement);
            ui.livePlayer.load();
        } else {
            ui.livePlayer.src = objectUrl;
        }
        return { mediaSource, objectUrl, sourceElement };
    }

    function isInitSegment(bytes) {
        const value = new Uint8Array(bytes);
        return value.length >= 8 &&
            value[4] === 0x66 && value[5] === 0x74 &&
            value[6] === 0x79 && value[7] === 0x70;
    }

    function enqueueLive(bytes) {
        const item = bytes instanceof ArrayBuffer ? bytes : bytes.buffer;
        while (liveQueue.length >= 12 || liveQueueBytes + item.byteLength > 8 * 1024 * 1024) {
            const removed = liveQueue.shift();
            if (!removed) break;
            liveQueueBytes -= removed.byteLength;
        }
        liveQueue.push(item);
        liveQueueBytes += item.byteLength;
        pumpLive();
    }

    function pumpLive() {
        if (!liveSource || liveSource.readyState !== "open" ||
            !liveQueue.length || (liveBuffer && liveBuffer.updating)) return;
        if (!liveBuffer) {
            if (!isInitSegment(liveQueue[0])) {
                const removed = liveQueue.shift();
                liveQueueBytes -= removed.byteLength;
                return;
            }
            try {
                liveBuffer = liveSource.addSourceBuffer(PLAYBACK_MIME);
                liveBuffer.mode = "segments";
                liveBuffer.addEventListener("updateend", () => {
                    maintainLiveBuffer();
                    pumpLive();
                });
                liveBuffer.addEventListener("error", () => {
                    setLiveStatus("媒体缓冲错误");
                });
            } catch (error) {
                setLiveStatus(`无法创建媒体缓冲：${error.message || error}`);
                return;
            }
        }
        const item = liveQueue.shift();
        liveQueueBytes -= item.byteLength;
        try {
            liveBuffer.appendBuffer(item);
        } catch (error) {
            console.warn("live append failed", error);
            setLiveStatus("实时流追加失败");
        }
    }

    function maintainLiveBuffer() {
        if (!liveBuffer || liveBuffer.updating || !ui.livePlayer.buffered.length) return;
        const ranges = ui.livePlayer.buffered;
        const start = ranges.start(0);
        const end = ranges.end(ranges.length - 1);
        const available = end - start;
        const behind = 1.5;
        const startBuffer = 3.0;
        if (!liveStarted && available >= startBuffer) {
            ui.livePlayer.currentTime = Math.max(start, end - behind);
            liveStarted = true;
            ui.livePlayer.play().catch(() => {
                setLiveStatus("流已就绪，请点击播放器开始");
            });
        } else if (liveStarted) {
            const lag = end - Number(ui.livePlayer.currentTime || 0);
            const maxLag = 7.0;
            if (lag > maxLag || ui.livePlayer.currentTime < start) {
                ui.livePlayer.currentTime = Math.max(start, end - behind);
            }
        }
        const lag = liveStarted ? Math.max(0, end - ui.livePlayer.currentTime) : available;
        setLiveStatus(`fMP4/MSE 直播 · 缓冲 ${lag.toFixed(1)}s`, true);
        const trimBefore = ui.livePlayer.currentTime - 30;
        if (trimBefore > start + 1) {
            try { liveBuffer.remove(start, trimBefore); } catch (_) {}
        }
    }

    async function startHubLive() {
        await stopHubLive();
        if (!state.device) {
            setLiveStatus("暂无在线设备");
            return;
        }
        if (!supportsMse()) {
            setLiveStatus("当前浏览器不支持 fMP4/MSE 直播");
            return;
        }
        liveMode = "mse";
        setLiveButtons(liveMode);
        ui.livePlayer.hidden = false;
        ui.moqPlayer.hidden = true;
        const session = ++liveSession;
        setLiveStatus("正在初始化播放器…");
        try {
            liveAttachment = attachLiveSource();
            liveSource = liveAttachment.mediaSource;
            if (!await waitForSourceOpen(liveSource) || session !== liveSession) return;
            if (liveAttachment.objectUrl) {
                URL.revokeObjectURL(liveAttachment.objectUrl);
                liveAttachment.objectUrl = "";
            }
        } catch (error) {
            liveMode = "";
            setLiveButtons();
            setLiveStatus(`播放器初始化失败：${error.message || error}`);
            return;
        }
        const protocol = location.protocol === "https:" ? "wss:" : "ws:";
        const url = `${protocol}//${location.host}/api/v1/devices/` +
            `${encodeURIComponent(state.device)}/live`;
        const socket = new WebSocket(url);
        liveSocket = socket;
        socket.binaryType = "arraybuffer";
        setLiveStatus("正在连接 camera-hub 实时流…");
        socket.onopen = () => {
            if (socket === liveSocket) setLiveStatus("已连接，等待媒体…");
        };
        socket.onmessage = (event) => {
            if (socket !== liveSocket || session !== liveSession) return;
            if (event.data instanceof ArrayBuffer) enqueueLive(event.data);
        };
        socket.onerror = () => {
            if (socket === liveSocket) setLiveStatus("实时流连接失败");
        };
        socket.onclose = () => {
            if (socket !== liveSocket) return;
            liveSocket = null;
            liveMode = "";
            setLiveButtons();
            setLiveStatus("实时流已断开");
        };
    }

    async function startHubFlv() {
        await stopHubLive();
        if (!state.device) {
            setLiveStatus("暂无在线设备");
            return;
        }
        if (!window.CameraHubFlv) {
            setLiveStatus("HTTP-FLV 播放组件尚未加载");
            return;
        }
        const session = ++liveSession;
        liveMode = "flv";
        setLiveButtons(liveMode);
        ui.livePlayer.hidden = false;
        ui.moqPlayer.hidden = true;
        setLiveStatus("正在连接 HTTP-FLV 流…");
        try {
            const url = `/api/v1/devices/${encodeURIComponent(state.device)}/live.flv`;
            window.CameraHubFlv.start(ui.livePlayer, url, (status, detail) => {
                if (session !== liveSession || liveMode !== "flv") return;
                if (status === "playing") {
                    setLiveStatus("HTTP-FLV 直播 · TCP", true);
                } else if (status === "ready") {
                    setLiveStatus("HTTP-FLV 已就绪，请点击播放器开始", true);
                } else if (status === "buffering") {
                    setLiveStatus("HTTP-FLV 缓冲中…", true);
                } else if (status === "error") {
                    setLiveStatus(`HTTP-FLV 播放失败：${detail || "未知错误"}`);
                }
            });
        } catch (error) {
            try { window.CameraHubFlv.stop(); } catch (_) {}
            liveMode = "";
            setLiveButtons();
            setLiveStatus(`HTTP-FLV 启动失败：${error.message || error}`);
        }
    }

    function waitForIceGathering(pc, timeoutMs = 3500) {
        if (pc.iceGatheringState === "complete") return Promise.resolve();
        return new Promise((resolve) => {
            const timeout = setTimeout(done, timeoutMs);
            function done() {
                clearTimeout(timeout);
                pc.removeEventListener("icegatheringstatechange", changed);
                resolve();
            }
            function changed() {
                if (pc.iceGatheringState === "complete") done();
            }
            pc.addEventListener("icegatheringstatechange", changed);
        });
    }

    async function startHubWebRtc() {
        await stopHubLive();
        if (!state.device) {
            setLiveStatus("暂无在线设备");
            return;
        }
        if (typeof RTCPeerConnection !== "function") {
            setLiveStatus("当前浏览器不支持 WebRTC");
            return;
        }
        const deviceId = state.device;
        const session = ++liveSession;
        liveMode = "webrtc";
        setLiveButtons(liveMode);
        ui.livePlayer.hidden = false;
        ui.moqPlayer.hidden = true;
        const peer = new RTCPeerConnection({
            iceCandidatePoolSize: 0,
            bundlePolicy: "max-bundle",
            rtcpMuxPolicy: "require",
        });
        livePeer = peer;
        const stream = new MediaStream();
        ui.livePlayer.srcObject = stream;
        peer.addTransceiver("video", { direction: "recvonly" });
        peer.addTransceiver("audio", { direction: "recvonly" });
        peer.ontrack = (event) => {
            if (peer !== livePeer || session !== liveSession) return;
            if (!stream.getTracks().some((track) => track.id === event.track.id)) {
                stream.addTrack(event.track);
            }
            ui.livePlayer.play().catch(() => setLiveStatus("画面已连接，请点击播放器开始", true));
        };
        peer.onconnectionstatechange = () => {
            if (peer !== livePeer || session !== liveSession) return;
            if (peer.connectionState === "connected") {
                setLiveStatus("WebRTC 直播 · 低延时", true);
            } else if (peer.connectionState === "failed") {
                setLiveStatus("WebRTC 连接失败，可切换 fMP4/MSE 直播");
            } else if (peer.connectionState === "disconnected") {
                setLiveStatus("WebRTC 暂时断开");
            } else {
                setLiveStatus("WebRTC ICE/DTLS 连接中…");
            }
        };
        setLiveStatus("正在创建 camera-hub WebRTC 会话…");
        try {
            const offer = await peer.createOffer();
            await peer.setLocalDescription(offer);
            await waitForIceGathering(peer);
            if (peer !== livePeer || session !== liveSession) return;
            const response = await fetch(
                `/api/v1/devices/${encodeURIComponent(deviceId)}/webrtc/offer`,
                {
                    method: "POST",
                    cache: "no-store",
                    headers: { "Content-Type": "application/json" },
                    body: JSON.stringify({ sdp: peer.localDescription.sdp }),
                },
            );
            if (!response.ok) throw new Error(await response.text());
            const body = await response.json();
            await peer.setRemoteDescription({ type: "answer", sdp: body.sdp });
            setLiveStatus("WebRTC ICE/DTLS 连接中…");
        } catch (error) {
            try { peer.close(); } catch (_) {}
            if (livePeer === peer) livePeer = null;
            liveMode = "";
            setLiveButtons();
            setLiveStatus(`WebRTC 协商失败：${error.message || error}`);
        }
    }

    async function startHubMoq() {
        await stopHubLive();
        if (!state.device) {
            setLiveStatus("暂无在线设备");
            return;
        }
        if (location.protocol !== "https:" || !window.isSecureContext) {
            setLiveStatus("MoQ 需要通过 HTTPS 打开本页面");
            return;
        }
        if (!window.CameraHubMoq) {
            setLiveStatus("MoQ 播放组件尚未加载");
            return;
        }
        try {
            const body = await api("/api/v1/moq/status");
            const moq = body && body.moq || {};
            if (!moq.enabled || !moq.running) {
                throw new Error(moq.last_error || "MoQ 服务未启动");
            }
            let host = location.hostname;
            if (host.includes(":") && !host.startsWith("[")) host = `[${host}]`;
            const session = ++liveSession;
            liveMode = "moq";
            setLiveButtons(liveMode);
            ui.livePlayer.hidden = true;
            ui.moqPlayer.hidden = false;
            setLiveStatus("正在建立 MoQ/WebTransport 会话…");
            window.CameraHubMoq.start(
                ui.moqPlayer,
                {
                    url: `https://${host}:443/?token=${encodeURIComponent(moq.auth_token || "")}`,
                    name: `${state.device}.msf`,
                    fingerprints: Array.isArray(moq.fingerprints) ? moq.fingerprints : [],
                    latencyMinMs: 200,
                    latencyMaxMs: 600,
                },
                (status) => {
                    if (session !== liveSession || liveMode !== "moq") return;
                    if (status === "connected") {
                        setLiveStatus("MoQ 直播 · 低延时", true);
                    } else if (status === "connecting") {
                        setLiveStatus("MoQ/WebTransport 连接中…");
                    } else {
                        setLiveStatus("MoQ 会话已断开");
                    }
                },
            );
        } catch (error) {
            ui.livePlayer.hidden = false;
            ui.moqPlayer.hidden = true;
            liveMode = "";
            setLiveButtons();
            setLiveStatus(`MoQ 启动失败：${error.message || error}`);
        }
    }

    async function stopHubLive() {
        const deviceId = state.device;
        const wasWebRtc = liveMode === "webrtc";
        liveSession += 1;
        const socket = liveSocket;
        liveSocket = null;
        try { window.CameraHubFlv && window.CameraHubFlv.stop(); } catch (_) {}
        try { window.CameraHubMoq && window.CameraHubMoq.stop(); } catch (_) {}
        ui.moqPlayer.replaceChildren();
        ui.moqPlayer.hidden = true;
        ui.livePlayer.hidden = false;
        if (socket) {
            try { socket.close(); } catch (_) {}
        }
        try {
            if (liveSource && liveSource.readyState === "open") {
                liveSource.endOfStream();
            }
        } catch (_) {}
        if (liveAttachment) {
            if (liveAttachment.objectUrl) {
                try { URL.revokeObjectURL(liveAttachment.objectUrl); } catch (_) {}
            }
            if (liveAttachment.sourceElement) liveAttachment.sourceElement.remove();
        }
        const peer = livePeer;
        livePeer = null;
        if (peer) {
            try { peer.close(); } catch (_) {}
        }
        ui.livePlayer.pause();
        if (ui.livePlayer.srcObject) {
            try { ui.livePlayer.srcObject.getTracks().forEach((track) => track.stop()); } catch (_) {}
            ui.livePlayer.srcObject = null;
        }
        ui.livePlayer.removeAttribute("src");
        ui.livePlayer.load();
        liveAttachment = null;
        liveSource = null;
        liveBuffer = null;
        liveQueue = [];
        liveQueueBytes = 0;
        liveMode = "";
        liveStarted = false;
        setLiveButtons();
        setLiveStatus("尚未开始");
        if (wasWebRtc && deviceId) {
            try {
                await fetch(
                    `/api/v1/devices/${encodeURIComponent(deviceId)}/webrtc/offer`,
                    { method: "DELETE", cache: "no-store", keepalive: true },
                );
            } catch (_) {}
        }
    }

    function handleError(error) {
        const message = error && error.message ? error.message : "未知错误";
        setConnection("error", "连接异常");
        showToast(`刷新失败：${message}`, true);
    }

    function setConnection(status, text) {
        ui.connection.dataset.state = status;
        ui.connection.querySelector("span").textContent = text;
    }

    function showToast(message, error = false) {
        clearTimeout(state.toastTimer);
        ui.toast.textContent = message;
        ui.toast.classList.toggle("error", error);
        ui.toast.classList.add("visible");
        state.toastTimer = setTimeout(() => ui.toast.classList.remove("visible"), 2400);
    }

    function formatBytes(value) {
        let number = Math.max(0, Number(value || 0));
        const units = ["B", "KiB", "MiB", "GiB", "TiB"];
        let unit = 0;
        while (number >= 1024 && unit < units.length - 1) {
            number /= 1024;
            unit += 1;
        }
        return `${number.toFixed(unit === 0 || number >= 100 ? 0 : 1)} ${units[unit]}`;
    }

    function formatDuration(seconds) {
        const value = Math.max(0, Math.floor(seconds));
        const days = Math.floor(value / 86400);
        const hours = Math.floor((value % 86400) / 3600);
        const minutes = Math.floor((value % 3600) / 60);
        return days ? `${days}天 ${hours}时` : hours ? `${hours}时 ${minutes}分` : `${minutes}分`;
    }

    function formatAge(seconds) {
        const value = Math.max(0, Math.floor(Number(seconds || 0)));
        return value < 5 ? "刚刚" : value < 60 ? `${value} 秒前` : `${Math.floor(value / 60)} 分钟前`;
    }

    function formatDate(value) {
        return /^\d{8}$/.test(value || "")
            ? `${value.slice(0, 4)}-${value.slice(4, 6)}-${value.slice(6, 8)}`
            : value || "--";
    }

    function formatTimestamp(epoch) {
        return Number(epoch)
            ? new Date(Number(epoch) * 1000).toLocaleString("zh-CN", { hour12: false })
            : "--";
    }

    function esc(value) {
        return String(value == null ? "" : value)
            .replaceAll("&", "&amp;")
            .replaceAll("<", "&lt;")
            .replaceAll(">", "&gt;")
            .replaceAll('"', "&quot;")
            .replaceAll("'", "&#039;");
    }

    ui.refresh.addEventListener("click", () => refreshAll());
    ui.settingsForm.addEventListener("input", () => {
        state.settingsDirty = true;
        setSettingsStatus("有未保存更改");
    });
    ui.settingsForm.addEventListener("submit", saveSettings);
    ui.reloadSettings.addEventListener("click", async () => {
        state.settingsDirty = false;
        await refreshAll(true);
    });
    ui.voiceForm.addEventListener("input", () => {
        state.voiceDirty = true;
    });
    ui.voicePlaybackVolume.addEventListener("input", () => {
        ui.voicePlaybackVolumeValue.textContent = `${ui.voicePlaybackVolume.value}%`;
    });
    ui.voiceForm.addEventListener("submit", saveVoice);
    ui.reloadVoice.addEventListener("click", async () => {
        state.voiceDirty = false;
        await loadVoice();
    });
    ui.addVoiceCommand.addEventListener("click", addVoiceCommand);
    ui.voiceCommandList.addEventListener("click", (event) => {
        const button = event.target.closest("[data-voice-action]");
        if (!button) return;
        const row = button.closest(".voice-command");
        if (!row) return;
        const id = row.querySelector('[data-voice-field="id"]').value;
        if (button.dataset.voiceAction === "delete") {
            row.remove();
            state.voiceDirty = true;
        } else if (button.dataset.voiceAction === "reply") {
            testVoiceCommand(id, false);
        } else if (button.dataset.voiceAction === "request") {
            testVoiceCommand(id, true);
        }
    });
    ui.qqForm.addEventListener("input", () => {
        state.qqDirty = true;
        updateQqCredentialRequirements();
    });
    ui.qqForm.addEventListener("submit", (event) => saveQqConfig(event));
    ui.reloadQq.addEventListener("click", async () => {
        state.qqDirty = false;
        await loadQq();
    });
    ui.clearQqSecret.addEventListener("click", async () => {
        if (!state.qqConfig?.secret_configured) return;
        if (!window.confirm("清除 AppSecret 后机器人会立即离线。继续？")) return;
        await saveQqConfig(null, true);
    });
    ui.rotateQqPushToken.addEventListener("click", rotateQqPushToken);
    ui.copyQqPushToken.addEventListener("click", copyQqPushToken);
    ui.qqTestForm.addEventListener("submit", sendQqTest);
    ui.deviceSelect.addEventListener("change", () => selectDevice(ui.deviceSelect.value));
    ui.liveDeviceSelect.addEventListener("change", () => {
        selectDevice(ui.liveDeviceSelect.value);
    });
    ui.photoDeviceSelect.addEventListener("change", () => {
        selectDevice(ui.photoDeviceSelect.value);
    });
    ui.startSmoothLive.addEventListener("click", startHubLive);
    ui.startFlvLive.addEventListener("click", startHubFlv);
    ui.startLowLive.addEventListener("click", startHubWebRtc);
    ui.startMoqLive.addEventListener("click", startHubMoq);
    ui.stopLive.addEventListener("click", stopHubLive);
    ui.refreshPhotos.addEventListener("click", loadPhotos);
    ui.selectAllPhotos.addEventListener("click", selectAllPhotos);
    ui.clearPhotoSelection.addEventListener("click", clearPhotoSelection);
    ui.deleteSelectedPhotos.addEventListener("click", deleteSelectedPhotos);
    ui.clearPhotos.addEventListener("click", clearCurrentPhotos);
    ui.closePhoto.addEventListener("click", closePhoto);
    ui.deletePhoto.addEventListener("click", deleteCurrentPhoto);
    ui.photoLightbox.addEventListener("click", (event) => {
        if (event.target === ui.photoLightbox || event.target === ui.photoPreview) closePhoto();
    });
    ui.dateSelect.addEventListener("change", async () => {
        stopRecordPlayback();
        state.date = ui.dateSelect.value;
        await loadRecords();
    });
    ui.playbackSpeed.addEventListener("change", () => {
        desiredPlaybackRate = Number(ui.playbackSpeed.value) || 1;
        applyPlaybackRate();
    });
    ui.player.addEventListener("loadedmetadata", applyPlaybackRate);
    ui.player.addEventListener("play", applyPlaybackRate);
    ui.player.addEventListener("ratechange", () => {
        if (performance.now() - playbackReloadAt < 1500) return;
        const rate = Number(ui.player.playbackRate);
        if (!Number.isFinite(rate) || rate <= 0 ||
            Math.abs(rate - desiredPlaybackRate) < 0.01) return;
        desiredPlaybackRate = rate;
        const option = Array.from(ui.playbackSpeed.options)
            .find((item) => Math.abs(Number(item.value) - rate) < 0.01);
        if (option) ui.playbackSpeed.value = option.value;
        ui.player.muted = rate > 2;
    });
    ui.player.addEventListener("timeupdate", drawTimeline);
    ui.player.addEventListener("seeking", drawTimeline);

    let timelineDragging = false;
    ui.timeline.addEventListener("pointerdown", (event) => {
        timelineDragging = true;
        ui.timeline.setPointerCapture(event.pointerId);
        showTimelinePreview(timelineSecondsAt(event.clientX), event.clientX);
    });
    ui.timeline.addEventListener("pointermove", (event) => {
        if (timelineDragging) {
            showTimelinePreview(timelineSecondsAt(event.clientX), event.clientX);
        }
    });
    ui.timeline.addEventListener("pointerup", (event) => {
        if (!timelineDragging) return;
        timelineDragging = false;
        const seconds = timelineSecondsAt(event.clientX);
        try { ui.timeline.releasePointerCapture(event.pointerId); } catch (_) {}
        seekTimeline(seconds);
    });
    ui.timeline.addEventListener("pointercancel", () => {
        timelineDragging = false;
        state.timelinePreview = null;
        ui.timelineTip.hidden = true;
        drawTimeline();
    });
    ui.timeline.addEventListener("keydown", (event) => {
        const current = timelineCursorSeconds() ??
            (state.records[0] ? state.records[0].startSeconds : 0);
        let target = null;
        if (event.key === "ArrowLeft") target = current - 300;
        if (event.key === "ArrowRight") target = current + 300;
        if (event.key === "Home") target = 0;
        if (event.key === "End") target = 86399;
        if (target == null) return;
        event.preventDefault();
        seekTimeline(Math.max(0, Math.min(86399, target)));
    });

    function activateView(view, updateHash = true) {
        const target = [
            "overview",
            "live",
            "evaluation",
            "voice",
            "qq",
            "playback",
            "photos",
            "settings",
        ].includes(view)
            ? view : "overview";
        if (state.view === "live" && target !== "live") stopHubLive();
        if (state.view === "evaluation" && target !== "evaluation") {
            window.CameraHubEvaluation?.stop();
        }
        if (target !== "photos") closePhoto();
        state.view = target;
        document.querySelectorAll("[data-view]").forEach((section) => {
            section.hidden = section.dataset.view !== target;
        });
        document.querySelectorAll("[data-view-target]").forEach((button) => {
            const active = button.dataset.viewTarget === target;
            button.classList.toggle("active", active);
            button.setAttribute("aria-current", active ? "page" : "false");
        });
        const activeNavigation = document.querySelector(`[data-view-target="${target}"]`);
        if (activeNavigation && activeNavigation.parentElement) {
            const navigation = activeNavigation.parentElement;
            navigation.scrollLeft = Math.max(
                0,
                activeNavigation.offsetLeft -
                    (navigation.clientWidth - activeNavigation.offsetWidth) / 2,
            );
        }
        if (updateHash) history.replaceState(null, "", `#${target}`);
        if (target === "playback") requestAnimationFrame(resizeTimeline);
        if (target === "photos") loadPhotos();
        if (target === "voice") loadVoice(true);
        if (target === "qq") loadQq(true);
        if (target === "evaluation") window.CameraHubEvaluation?.refreshDevices();
        window.scrollTo({ top: 0, behavior: "smooth" });
    }

    document.querySelectorAll("[data-view-target]").forEach((button) => {
        button.addEventListener("click", () => activateView(button.dataset.viewTarget));
    });
    window.addEventListener("hashchange", () => {
        activateView(location.hash.slice(1), false);
    });
    window.addEventListener("resize", resizeTimeline);
    activateView(location.hash.slice(1) || "overview", false);
    requestAnimationFrame(resizeTimeline);

    updateLiveClock();
    setInterval(updateLiveClock, 50);
    setInterval(() => refreshAll(true), 10_000);
    setInterval(() => {
        if (state.view === "voice" && !state.voiceDirty) loadVoice(true);
        if (state.view === "qq" && !state.qqDirty) loadQq(true);
    }, 5_000);
    refreshAll(true);
})();
