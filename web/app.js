(() => {
    "use strict";

    const $ = (id) => document.getElementById(id);
    const ui = {
        cameraSubnav: $("cameraSubnav"),
        voiceSubnav: $("voiceSubnav"),
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
        voiceProfilePanel: $("voiceProfilePanel"),
        voiceState: $("voiceState"),
        voiceDetected: $("voiceDetected"),
        voiceAudioLevel: $("voiceAudioLevel"),
        voiceLastKeyword: $("voiceLastKeyword"),
        voiceTtsState: $("voiceTtsState"),
        voiceLastError: $("voiceLastError"),
        voiceAsrStatus: $("voiceAsrStatus"),
        voiceAsrState: $("voiceAsrState"),
        voiceAsrTranscript: $("voiceAsrTranscript"),
        voiceAsrError: $("voiceAsrError"),
        voiceTranscribeButton: $("voiceTranscribeButton"),
        voiceTranscribeWindow: $("voiceTranscribeWindow"),
        voiceForm: $("voiceForm"),
        voiceReferencePrompt: $("voiceReferencePrompt"),
        voiceReferenceStatus: $("voiceReferenceStatus"),
        voiceReferencePreview: $("voiceReferencePreview"),
        recordVoiceReference: $("recordVoiceReference"),
        stopVoiceReference: $("stopVoiceReference"),
        deleteVoiceReference: $("deleteVoiceReference"),
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
        voiceStudioStatus: $("voiceStudioStatus"),
        voiceStudioForm: $("voiceStudioForm"),
        voiceStudioEnabled: $("voiceStudioEnabled"),
        voiceStudioUrl: $("voiceStudioUrl"),
        openVoiceStudio: $("openVoiceStudio"),
        copyVoiceStudioUrl: $("copyVoiceStudioUrl"),
        voiceStudioSessionTtl: $("voiceStudioSessionTtl"),
        voiceStudioRetention: $("voiceStudioRetention"),
        voiceStudioReferenceLimit: $("voiceStudioReferenceLimit"),
        voiceStudioTextLimit: $("voiceStudioTextLimit"),
        saveVoiceStudio: $("saveVoiceStudio"),
        voiceServiceControlStatus: $("voiceServiceControlStatus"),
        voiceServiceControls: $("voiceServiceControls"),
        ttsServiceState: $("ttsServiceState"),
        ttsServiceDetail: $("ttsServiceDetail"),
        ttsServicePid: $("ttsServicePid"),
        voiceServiceState: $("voiceServiceState"),
        voiceServiceDetail: $("voiceServiceDetail"),
        voiceServicePid: $("voiceServicePid"),
        irStatus: $("irStatus"),
        irDetail: $("irDetail"),
        irDevice: $("irDevice"),
        irPid: $("irPid"),
        irServiceControls: $("irServiceControls"),
        irLastResult: $("irLastResult"),
        airconStateKnown: $("airconStateKnown"),
        airconStatePower: $("airconStatePower"),
        airconStateMode: $("airconStateMode"),
        airconStateTemperature: $("airconStateTemperature"),
        airconStateEco: $("airconStateEco"),
        airconRemotePower: $("airconRemotePower"),
        airconRemoteMode: $("airconRemoteMode"),
        airconRemoteTemperature: $("airconRemoteTemperature"),
        airconRemoteEco: $("airconRemoteEco"),
        airconPowerButton: $("airconPowerButton"),
        airconTempDown: $("airconTempDown"),
        airconTempUp: $("airconTempUp"),
        airconEcoButton: $("airconEcoButton"),
        airconTimerButton: $("airconTimerButton"),
        airconTimerPanel: $("airconTimerPanel"),
        airconTimerOperation: $("airconTimerOperation"),
        airconTimerHours: $("airconTimerHours"),
        airconTimerMinutes: $("airconTimerMinutes"),
        airconTimerSubmit: $("airconTimerSubmit"),
        airconRemoteStatus: $("airconRemoteStatus"),
        airconVerificationProgress: $("airconVerificationProgress"),
        airconVerificationList: $("airconVerificationList"),
        airconVerificationPrompt: $("airconVerificationPrompt"),
        airconVerificationPromptText: $("airconVerificationPromptText"),
        airconVerificationPass: $("airconVerificationPass"),
        airconVerificationFail: $("airconVerificationFail"),
        airconCommandForm: $("airconCommandForm"),
        airconOperation: $("airconOperation"),
        airconMode: $("airconMode"),
        airconTemperature: $("airconTemperature"),
        airconEco: $("airconEco"),
        airconDelayMinutes: $("airconDelayMinutes"),
        airconAutoOffMinutes: $("airconAutoOffMinutes"),
        sendAirconCommand: $("sendAirconCommand"),
        airconScheduleList: $("airconScheduleList"),
        cancelAllAirconSchedules: $("cancelAllAirconSchedules"),
        airconIntegrationStatus: $("airconIntegrationStatus"),
        airconIntegrationGate: $("airconIntegrationGate"),
        airconIntegrationGateText: $("airconIntegrationGateText"),
        enableAirconIntegration: $("enableAirconIntegration"),
        disableAirconIntegration: $("disableAirconIntegration"),
        airconIntegrationDetails: $("airconIntegrationDetails"),
        airconIntegrationToken: $("airconIntegrationToken"),
        rotateAirconToken: $("rotateAirconToken"),
        copyAirconToken: $("copyAirconToken"),
        airconTriggerUrl: $("airconTriggerUrl"),
        copyAirconTrigger: $("copyAirconTrigger"),
        airconUrlGuide: $("airconUrlGuide"),
        copyAirconGuide: $("copyAirconGuide"),
        qqStatus: $("qqStatus"),
        qqState: $("qqState"),
        qqBotName: $("qqBotName"),
        qqConnectedAt: $("qqConnectedAt"),
        qqSentCount: $("qqSentCount"),
        qqLastError: $("qqLastError"),
        qqMessageCount: $("qqMessageCount"),
        qqMessageList: $("qqMessageList"),
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
        qqCurlExample: $("qqCurlExample"),
        copyQqCurlExample: $("copyQqCurlExample"),
        qqTestForm: $("qqTestForm"),
        qqTestTarget: $("qqTestTarget"),
        qqTestMessage: $("qqTestMessage"),
        sendQqTest: $("sendQqTest"),
        ddnsStatus: $("ddnsStatus"),
        ddnsState: $("ddnsState"),
        ddnsStableIpv6: $("ddnsStableIpv6"),
        ddnsPrefix: $("ddnsPrefix"),
        ddnsLastSuccess: $("ddnsLastSuccess"),
        ddnsNextAttempt: $("ddnsNextAttempt"),
        ddnsChangedCount: $("ddnsChangedCount"),
        ddnsLastError: $("ddnsLastError"),
        ddnsProcessState: $("ddnsProcessState"),
        ddnsProcessPid: $("ddnsProcessPid"),
        ddnsServiceControls: $("ddnsServiceControls"),
        ddnsForm: $("ddnsForm"),
        ddnsEnabled: $("ddnsEnabled"),
        ddnsDomain: $("ddnsDomain"),
        ddnsSecretId: $("ddnsSecretId"),
        ddnsSecretKey: $("ddnsSecretKey"),
        ddnsSecretState: $("ddnsSecretState"),
        ddnsInterface: $("ddnsInterface"),
        ddnsTtl: $("ddnsTtl"),
        ddnsInterval: $("ddnsInterval"),
        ddnsForceInterval: $("ddnsForceInterval"),
        ddnsRecordList: $("ddnsRecordList"),
        addDdnsRecord: $("addDdnsRecord"),
        reloadDdns: $("reloadDdns"),
        clearDdnsSecret: $("clearDdnsSecret"),
        previewDdns: $("previewDdns"),
        saveDdns: $("saveDdns"),
        reconcileDdns: $("reconcileDdns"),
        ddnsPreview: $("ddnsPreview"),
        httpProxyStatus: $("httpProxyStatus"),
        httpProxyForm: $("httpProxyForm"),
        httpProxyEnabled: $("httpProxyEnabled"),
        reloadHttpProxy: $("reloadHttpProxy"),
        saveHttpProxy: $("saveHttpProxy"),
        httpProxyAccess: $("httpProxyAccess"),
        httpProxyToken: $("httpProxyToken"),
        rotateHttpProxyToken: $("rotateHttpProxyToken"),
        copyHttpProxyToken: $("copyHttpProxyToken"),
        httpProxyTemplate: $("httpProxyTemplate"),
        copyHttpProxyTemplate: $("copyHttpProxyTemplate"),
        httpProxyTestForm: $("httpProxyTestForm"),
        httpProxyTestMethod: $("httpProxyTestMethod"),
        httpProxyTestUrl: $("httpProxyTestUrl"),
        httpProxyTestHeaders: $("httpProxyTestHeaders"),
        httpProxyTestBody: $("httpProxyTestBody"),
        testHttpProxy: $("testHttpProxy"),
        httpProxyTestResult: $("httpProxyTestResult"),
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
        photoAiStatus: $("photoAiStatus"),
        photoGrid: $("photoGrid"),
        photoEmpty: $("photoEmpty"),
        photoLightbox: $("photoLightbox"),
        photoPreview: $("photoPreview"),
        downloadPhoto: $("downloadPhoto"),
        deletePhoto: $("deletePhoto"),
        closePhoto: $("closePhoto"),
        toast: $("toast"),
    };
    const tvUi = Object.fromEntries([
        "tvStatus", "tvMessage", "tvLastChecked", "tvLastRecovered", "tvRecoveryCount",
        "tvNextRetry", "tvError", "checkTv", "reloadTv", "tvLinks", "tvControlLink",
        "tvGuideLink", "copyTvGuide", "tvForm", "tvEnabled", "tvHost", "tvAdbPort",
        "tvPollSeconds", "tvQuality", "tvFrameInterval", "tvApiOrigin", "saveTv",
        "tvApkStatus", "tvApkForm", "tvApkFile", "uploadTvApk",
    ].map((id) => [id, $(id)]));
    const state = {
        devices: [],
        media: [],
        days: [],
        records: [],
        photos: [],
        ai: null,
        photoSelection: new Set(),
        currentPhoto: "",
        device: "",
        date: "",
        view: "overview",
        cameraView: "live",
        voiceView: "commands",
        transcribeBusy: false,
        currentRecord: "",
        segmentSeconds: 600,
        timelinePreview: null,
        voiceConfig: null,
        voiceDirty: false,
        voiceBusy: false,
        voiceRecorder: null,
        voiceStream: null,
        voiceChunks: [],
        voiceRecordTimer: 0,
        voicePreviewUrl: "",
        voiceProfileBusy: false,
        voiceStudio: null,
        voiceStudioDirty: false,
        voiceServicesBusy: false,
        voiceAssets: null,
        voiceAssetsBusy: false,
        voiceAssetsInstalling: false,
        irBusy: false,
        airconBusy: false,
        aircon: null,
        airconDirty: false,
        airconAvailable: false,
        airconPendingVerification: null,
        qqConfig: null,
        qqDirty: false,
        qqBusy: false,
        ddnsConfig: null,
        ddnsDirty: false,
        ddnsBusy: false,
        ddnsComponentBusy: false,
        ddnsWorkerOnline: false,
        httpProxy: null,
        httpProxyDirty: false,
        httpProxyBusy: false,
        tv: null,
        tvDirty: false,
        tvBusy: false,
        busy: false,
        settingsDirty: false,
        toastTimer: 0,
    };
    const CAMERA_VIEWS = new Set([
        "live",
        "playback",
        "photos",
        "settings",
        "evaluation",
    ]);
    const VOICE_VIEWS = new Set(["commands", "profile", "public", "status", "aircon"]);
    const ROOT_VIEWS = new Set(["overview", "voice", "qq", "ddns", "proxy", "tv"]);

    // Android 9 devices may ship WebView versions without structuredClone.
    const cloneJson = (value) => {
        if (value == null) return value;
        if (typeof structuredClone === "function") return structuredClone(value);
        return JSON.parse(JSON.stringify(value));
    };

    const clearChildren = (element) => {
        while (element.firstChild) element.removeChild(element.firstChild);
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
        state.ai = ai;
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
        ui.aiRuntimeStatus.textContent = aiStatusText(ai);
        ui.aiRuntimeStatus.classList.toggle("error", !!(ai.last_error || ai.last_snapshot_error));
        renderPhotoAiStatus();
    }

    function aiStatusText(ai) {
        if (!ai.enabled) return "AI 已关闭";
        if (!ai.available || !ai.running) return ai.last_error || "AI 运行时不可用";
        const error = ai.last_snapshot_error || ai.last_error;
        return `运行中 · 推理 ${ai.inference_count || 0} 次 · ` +
            `成功抓拍 ${ai.detection_count || 0} 次 · ` +
            `最高人物置信度 ${(Number(ai.last_confidence || 0) * 100).toFixed(1)}%` +
            `（阈值 ${(Number(ai.threshold || 0) * 100).toFixed(0)}%）` +
            ` · ${Number(ai.last_inference_ms || 0).toFixed(0)} ms/次` +
            (error ? ` · 抓拍失败 ${ai.snapshot_failure_count || 0} 次 · ${error}` : "");
    }

    function renderPhotoAiStatus() {
        const ai = state.ai;
        ui.photoAiStatus.textContent = ai ? aiStatusText(ai) : "等待 AI 状态";
        ui.photoAiStatus.classList.toggle("error", !!(ai?.last_error || ai?.last_snapshot_error));
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
            const [body, servicesBody, assetsBody, irBody] = await Promise.all([
                api("/api/v1/voice"),
                api("/api/v1/components"),
                api("/api/v1/assets"),
                api("/api/v1/ir"),
            ]);
            renderVoice(body);
            renderVoiceAssets(assetsBody && assetsBody.assets || {});
            renderVoiceServices(servicesBody && servicesBody.components || {});
            renderIr(
                irBody && irBody.ir || {},
                servicesBody && servicesBody.components || {},
                irBody && irBody.aircon || {},
            );
        } catch (error) {
            if (!silent) handleError(error);
        } finally {
            state.voiceBusy = false;
        }
    }

    const VOICE_ASR_STATES = {
        missing: "模型未安装",
        idle: "空闲",
        loading: "正在加载模型",
        listening: "正在聆听",
        ready: "识别完成",
        error: "识别异常",
    };

    function renderVoiceAsr(status, online) {
        if (!ui.voiceAsrStatus) return;
        const available = !!status.asr_available;
        const asrState = status.asr_state || (available ? "idle" : "missing");
        const listening = asrState === "listening" || asrState === "loading";
        ui.voiceAsrStatus.textContent = !available
            ? "模型未安装"
            : listening
                ? "识别中"
                : "就绪";
        ui.voiceAsrStatus.className = `chip ${available ? "active" : "offline"}`;
        ui.voiceAsrState.textContent = VOICE_ASR_STATES[asrState] || asrState;
        ui.voiceAsrTranscript.textContent = status.asr_transcript || "--";
        ui.voiceAsrError.textContent =
            status.asr_error || (available ? "运行正常" : "请先在服务状态中安装自然语言识别模型");
        ui.voiceAsrError.classList.toggle("error", !!status.asr_error);
        if (ui.voiceTranscribeButton) {
            const busy = state.transcribeBusy || listening;
            ui.voiceTranscribeButton.disabled = !online || !available || busy;
            ui.voiceTranscribeButton.textContent = busy ? "识别中…" : "开始转写";
        }
    }

    function renderVoice(body) {
        const config = body && body.config || {};
        const status = body && body.status || {};
        const serverEpoch = Number(body && body.server_epoch) || Date.now() / 1000;
        const stale = !status.updated_epoch ||
            serverEpoch - Number(status.updated_epoch) > 15;
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
        const profileRevision = Number(config.voice_profile_revision || 0);
        ui.voiceTtsState.textContent = status.tts_state ||
            (profileRevision > 0 ? "声纹已录入" : "系统声音");
        ui.voiceLastError.textContent =
            status.last_error || (online ? "运行正常" : "等待 worker 状态");
        ui.voiceLastError.classList.toggle("error", !!status.last_error);
        renderVoiceAsr(status, online);
        ui.voiceReferencePrompt.textContent = body.reference_prompt || "--";
        if (!state.voiceProfileBusy && !state.voiceRecorder) {
            ui.voiceReferenceStatus.textContent = profileRevision > 0
                ? `声纹已录入 · 版本 ${profileRevision} · 修改后将重新生成全部回复`
                : "尚未录入，继续使用系统声音";
            ui.deleteVoiceReference.disabled = profileRevision === 0;
        }

        if (!state.voiceDirty && !state.voiceProfileBusy) {
            state.voiceConfig = cloneJson(config);
            ui.voiceEnabled.checked = !!config.enabled;
            ui.voiceCaptureDevice.value = config.capture_device || "hw:0,0";
            ui.voicePlaybackDevice.value = config.playback_device || "plughw:0,0";
            const playbackVolume = Number(config.playback_volume ?? 60);
            ui.voicePlaybackVolume.value = playbackVolume;
            ui.voicePlaybackVolumeValue.textContent = `${playbackVolume}%`;
            ui.voiceRequestTimeout.value = config.request_timeout_ms || 3000;
            ui.voiceGlobalCooldown.value = config.global_cooldown_ms || 500;
            ui.voiceFailureReply.value = config.failure_reply || "操作失败，请稍后再试";
            renderVoiceCommands(Array.isArray(config.commands) ? config.commands : []);
        }
        renderVoiceStudio(body && body.public_studio || {});
        renderVoiceEvents(Array.isArray(body.events) ? body.events : []);
    }

    function renderVoiceStudio(publicStudio) {
        const enabled = publicStudio.enabled !== false;
        const url = publicStudio.url || `${location.origin}/voice-studio`;
        state.voiceStudio = cloneJson(publicStudio);
        ui.voiceStudioStatus.textContent = enabled ? "已开放" : "已关闭";
        ui.voiceStudioStatus.className = `chip ${enabled ? "active" : "offline"}`;
        if (!state.voiceStudioDirty) {
            ui.voiceStudioEnabled.checked = enabled;
        }
        ui.voiceStudioUrl.value = url;
        ui.openVoiceStudio.href = enabled ? url : "#";
        ui.openVoiceStudio.classList.toggle("disabled", !enabled);
        ui.openVoiceStudio.setAttribute("aria-disabled", enabled ? "false" : "true");
        ui.copyVoiceStudioUrl.disabled = !url;
        ui.voiceStudioSessionTtl.textContent =
            `${Number(publicStudio.session_ttl_hours || 24)} 小时`;
        ui.voiceStudioRetention.textContent =
            `${Number(publicStudio.profile_retention_hours || 24)} 小时`;
        const referenceSeconds = publicStudio.reference_seconds || {};
        ui.voiceStudioReferenceLimit.textContent =
            `${Number(referenceSeconds.min || 3)}–${Number(referenceSeconds.max || 20)} 秒`;
        ui.voiceStudioTextLimit.textContent =
            `最多 ${Number(publicStudio.max_text_chars || 120)} 字`;
    }

    async function saveVoiceStudioSettings(event) {
        event.preventDefault();
        ui.saveVoiceStudio.disabled = true;
        try {
            await api("/api/v1/voice-studio/settings", {
                method: "PUT",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ enabled: ui.voiceStudioEnabled.checked }),
            });
            state.voiceStudioDirty = false;
            showToast(
                ui.voiceStudioEnabled.checked
                    ? "公共语音工作室已开放"
                    : "公共语音工作室已关闭",
            );
            await loadVoice(true);
        } catch (error) {
            handleError(error);
        } finally {
            ui.saveVoiceStudio.disabled = false;
        }
    }

    async function copyVoiceStudioUrl() {
        const value = ui.voiceStudioUrl.value;
        if (!value) return;
        try {
            await navigator.clipboard.writeText(value);
        } catch (_) {
            ui.voiceStudioUrl.select();
            document.execCommand("copy");
        }
        showToast("公共访问链接已复制");
    }

    function renderVoiceServices(services) {
        const available = !!services.control_available;
        ui.voiceServiceControlStatus.textContent = available ? "控制可用" : "仅状态";
        ui.voiceServiceControlStatus.className = `chip ${available ? "active" : ""}`.trim();
        renderManagedVoiceService("tts", services.tts || {}, available);
        renderManagedVoiceService("voice", services.voice || {}, available);
    }

    function renderIr(ir, components, aircon) {
        const service = components.ir || {};
        const available = !!ir.available && !!service.healthy;
        ui.irStatus.textContent = available
            ? "发射就绪"
            : service.installed === false ? "设备不可用" : "服务离线";
        ui.irStatus.className = `chip ${available ? "active" : "offline"}`;
        ui.irDetail.textContent = ir.detail || service.detail || "--";
        ui.irDevice.textContent = ir.device || "--";
        ui.irPid.textContent = service.pid ? String(service.pid) : "--";
        const autostart = ui.irServiceControls
            .querySelector('[data-service-autostart="ir"]');
        autostart.checked = !!service.autostart;
        autostart.disabled = state.irBusy || !components.control_available;
        ui.irServiceControls.querySelectorAll("[data-service-control]").forEach((button) => {
            const action = button.dataset.serviceAction;
            button.disabled = state.irBusy ||
                !components.control_available ||
                !service.installed ||
                (action === "start" ? service.running : !service.running);
        });
        state.airconAvailable = available;
        renderAircon(aircon, available);
    }

    const AIRCON_MODE_LABELS = {
        cool: "制冷",
        dry: "抽湿",
        heat: "制热",
        auto: "自动",
    };

    const AIRCON_CAPABILITIES = [
        ["power_on", "开机"],
        ["power_off", "关机"],
        ["cool", "制冷模式"],
        ["dry", "抽湿模式"],
        ["heat", "制热模式"],
        ["auto", "自动模式"],
        ["temperature_up", "升高温度"],
        ["temperature_down", "降低温度"],
        ["eco_on", "开启 ECO"],
        ["eco_off", "关闭 ECO"],
    ];

    const AIRCON_SCHEDULE_STATUS = {
        pending: "等待执行",
        running: "正在执行",
        completed: "已完成",
        failed: "失败",
        cancelled: "已取消",
    };

    function renderAircon(aircon, available) {
        state.aircon = cloneJson(aircon || {});
        const current = aircon.state || {};
        ui.airconStateKnown.textContent = aircon.state_known ? "已有发送记录" : "使用默认值";
        ui.airconStatePower.textContent = current.power ? "开启" : "关闭";
        ui.airconStateMode.textContent = AIRCON_MODE_LABELS[current.mode] || current.mode || "--";
        ui.airconStateTemperature.textContent =
            Number(current.temperature_c) ? `${current.temperature_c}°C` : "--";
        ui.airconStateEco.textContent = current.eco ? "开启" : "关闭";
        if (!state.airconDirty) {
            ui.airconMode.value = current.mode || "cool";
            ui.airconTemperature.value = current.temperature_c || 26;
            ui.airconEco.checked = !!current.eco;
        }
        ui.sendAirconCommand.disabled = state.airconBusy || !available;
        renderAirconSchedules(Array.isArray(aircon.schedules) ? aircon.schedules : []);
        renderAirconRemote(current, available);
        renderAirconVerification(aircon);
        renderAirconIntegration();
        updateAirconForm();
    }

    function renderAirconRemote(current, available) {
        const power = !!current.power;
        const temperature = Number(current.temperature_c) || 26;
        const mode = current.mode || "cool";
        const eco = !!current.eco;
        ui.airconRemotePower.textContent = power ? "ON" : "OFF";
        ui.airconRemoteMode.textContent = AIRCON_MODE_LABELS[mode] || mode;
        ui.airconRemoteTemperature.textContent = `${temperature}°C`;
        ui.airconRemoteEco.classList.toggle("active", eco);
        ui.airconPowerButton.classList.toggle("active", power);
        document.querySelectorAll("[data-aircon-mode]").forEach((button) => {
            button.classList.toggle("active", button.dataset.airconMode === mode);
            button.disabled = state.airconBusy || !available;
        });
        ui.airconPowerButton.disabled = state.airconBusy || !available;
        ui.airconTempDown.disabled = state.airconBusy || !available || temperature <= 17;
        ui.airconTempUp.disabled = state.airconBusy || !available || temperature >= 30;
        ui.airconEcoButton.disabled =
            state.airconBusy || !available || (!eco && mode !== "cool");
        ui.airconEcoButton.classList.toggle("active", eco);
        ui.airconTimerButton.disabled = state.airconBusy || !available;
        ui.airconTimerPanel.querySelectorAll("select, input, button").forEach((control) => {
            control.disabled = state.airconBusy || !available;
        });
        ui.airconRemoteStatus.textContent = available
            ? "操作会立即发送红外，请将 MI6 发射口对准空调。"
            : "红外服务未就绪，遥控器暂不可用。";
    }

    function renderAirconVerification(aircon) {
        const verified = new Set(
            Array.isArray(aircon.verified_capabilities)
                ? aircon.verified_capabilities
                : [],
        );
        const required = Array.isArray(aircon.required_capabilities)
            ? aircon.required_capabilities
            : AIRCON_CAPABILITIES.map(([capability]) => capability);
        const count = required.filter((capability) => verified.has(capability)).length;
        ui.airconVerificationProgress.textContent = `${count} / ${required.length}`;
        ui.airconVerificationProgress.className =
            `chip ${count === required.length ? "active" : ""}`.trim();
        ui.airconVerificationList.innerHTML = AIRCON_CAPABILITIES.map(
            ([capability, label]) => {
                const passed = verified.has(capability);
                return `<div class="aircon-verification-item ${passed ? "verified" : ""}">
                    <span>${esc(label)}</span>
                    <strong>${passed ? "已通过" : "待验证"}</strong>
                    ${passed
                        ? `<button class="button ghost" type="button"
                                   data-aircon-verification-reset="${capability}">重置</button>`
                        : ""}
                </div>`;
            },
        ).join("");
        const pending = state.airconPendingVerification;
        ui.airconVerificationPrompt.hidden = !pending;
        if (pending) {
            ui.airconVerificationPromptText.textContent =
                `“${pending.label}”红外已发送，实体空调是否实际生效？`;
        }
    }

    function formatAirconOperation(command) {
        if (!command) return "未知操作";
        if (command.operation === "set") {
            return `${AIRCON_MODE_LABELS[command.mode] || command.mode} ` +
                `${command.temperature_c}°C · ECO ${command.eco ? "开" : "关"}`;
        }
        if (command.operation === "off") return "关闭空调";
        if (command.operation === "power") return command.enabled ? "打开空调" : "关闭空调";
        if (command.operation === "set_temperature") return `设置 ${command.temperature_c}°C`;
        if (command.operation === "adjust_temperature") {
            return `温度${Number(command.delta_c) > 0 ? "+" : ""}${command.delta_c}°C`;
        }
        if (command.operation === "set_mode") {
            return `切换${AIRCON_MODE_LABELS[command.mode] || command.mode}`;
        }
        if (command.operation === "set_eco") return `${command.enabled ? "开启" : "关闭"} ECO`;
        return command.operation || "未知操作";
    }

    function renderAirconSchedules(schedules) {
        const pendingCount =
            schedules.filter((schedule) => schedule.status === "pending").length;
        ui.airconTimerButton.textContent = pendingCount ? `定时 · ${pendingCount}` : "定时";
        ui.airconTimerButton.classList.toggle("active", pendingCount > 0);
        if (!schedules.length) {
            ui.airconScheduleList.innerHTML = '<div class="empty">暂无定时任务</div>';
            ui.cancelAllAirconSchedules.disabled = true;
            return;
        }
        const pending = pendingCount > 0;
        ui.cancelAllAirconSchedules.disabled = state.airconBusy || !pending;
        ui.airconScheduleList.innerHTML = schedules.slice(0, 20).map((schedule) => `
            <div class="aircon-schedule-row">
                <span>
                    <strong>${esc(formatAirconOperation(schedule.command))}</strong>
                    <small>${esc(AIRCON_SCHEDULE_STATUS[schedule.status] || schedule.status)}
                        · ${esc(formatTimestamp(schedule.execute_at_epoch))}
                        ${schedule.message ? ` · ${esc(schedule.message)}` : ""}</small>
                </span>
                ${schedule.status === "pending"
                    ? `<button class="button danger" type="button"
                               data-aircon-schedule-cancel="${esc(schedule.id)}">取消</button>`
                    : ""}
            </div>`).join("");
    }

    function updateAirconForm() {
        const setting = ui.airconOperation.value === "set";
        const supportsEco = ui.airconMode.value === "cool";
        if (setting && !supportsEco) ui.airconEco.checked = false;
        ui.airconMode.disabled = !setting || state.airconBusy;
        ui.airconTemperature.disabled = !setting || state.airconBusy;
        ui.airconEco.disabled = !setting || !supportsEco || state.airconBusy;
        ui.airconAutoOffMinutes.disabled = !setting || state.airconBusy;
    }

    function airconCommandPayload() {
        const setting = ui.airconOperation.value === "set";
        const delay = Math.max(0, Number(ui.airconDelayMinutes.value || 0));
        const autoOff = Math.max(0, Number(ui.airconAutoOffMinutes.value || 0));
        return {
            command: setting
                ? {
                    operation: "set",
                    temperature_c: Number(ui.airconTemperature.value),
                    mode: ui.airconMode.value,
                    eco: ui.airconEco.checked,
                }
                : { operation: "off" },
            delay_minutes: delay || null,
            auto_off_minutes: setting && autoOff ? autoOff : null,
            request_id: "",
        };
    }

    function airconIntegrationEndpoint() {
        const aircon = state.aircon || {};
        return `${location.origin}` +
            `${aircon.integration_endpoint || "/api/v1/integrations/aircon/command"}`;
    }

    function airconTriggerUrl(parameters) {
        const aircon = state.aircon || {};
        const params = new URLSearchParams({
            token: aircon.integration_token || "AIRCON_TOKEN",
            ...parameters,
        });
        return `${airconIntegrationEndpoint()}?${params}`;
    }

    function airconUrlGuide() {
        return [
            "空调控制说明",
            "",
            "直接以 HTTPS GET 访问触发 URL；访问即执行控制。",
            `基础入口：${airconIntegrationEndpoint()}`,
            `认证参数：token=${state.aircon?.integration_token || "AIRCON_TOKEN"}`,
            "",
            "必填参数",
            "operation：set | off | power | set_temperature | adjust_temperature | " +
                "set_mode | set_eco | cancel_schedule | cancel_all",
            "",
            "操作参数",
            "set：temperature_c=17..30，mode=cool|dry|heat|auto，eco=true|false",
            "power / set_eco：enabled=true|false",
            "set_temperature：temperature_c=17..30",
            "adjust_temperature：delta_c=-10..-1 或 1..10",
            "set_mode：mode=cool|dry|heat|auto",
            "ECO 只支持制冷模式；切换到其他模式会自动关闭 ECO",
            "cancel_schedule：schedule_id=任务 ID",
            "",
            "可选调度参数",
            "delay_minutes=1..43200（延迟执行）",
            "execute_at_epoch=未来 30 天内的 Unix 秒时间戳；不能与 delay_minutes 同时使用",
            "auto_off_minutes=1..10080（开机操作执行后的自动关机）",
            "request_id=最长 64 位字母、数字、-、_、.；用于定时任务去重",
            "",
            "常用触发 URL",
            "制冷 24°C，关闭 ECO：",
            airconTriggerUrl({
                operation: "set", temperature_c: "24", mode: "cool", eco: "false",
            }),
            "关闭空调：",
            "operation=off",
            "打开空调（沿用影子状态）：",
            "operation=power&enabled=true",
            "设置 25°C：",
            "operation=set_temperature&temperature_c=25",
            "降低 2°C：",
            "operation=adjust_temperature&delta_c=-2",
            "切换抽湿模式：",
            "operation=set_mode&mode=dry",
            "30 分钟后关闭：",
            "operation=off&delay_minutes=30",
            "取消全部定时任务：",
            "operation=cancel_all",
            "",
            "注意",
            "1. Token 是控制凭据，请只交给可信助手。",
            "2. 相对调温依赖 camera-hub 的影子状态；超时后不要自动重试相对调温。",
            "3. 返回 physical_confirmed=false 表示红外已发送，但没有空调物理回执。",
        ].join("\n");
    }

    function renderAirconIntegration() {
        if (!ui.airconTriggerUrl || !ui.airconUrlGuide) return;
        const aircon = state.aircon || {};
        const enabled = !!aircon.integration_enabled;
        const complete = !!aircon.verification_complete;
        ui.airconIntegrationStatus.textContent = enabled
            ? "已开放"
            : complete ? "待启用" : "尚未开放";
        ui.airconIntegrationStatus.className =
            `chip ${enabled ? "active" : "offline"}`;
        ui.airconIntegrationGate.hidden = enabled;
        ui.airconIntegrationDetails.hidden = !enabled;
        ui.airconIntegrationGateText.textContent = complete
            ? "全部实体功能已通过，可生成新 Token 并开放外部接口。"
            : "尚有未验证功能，外部接口保持关闭。";
        ui.enableAirconIntegration.disabled = state.airconBusy || !complete;
        ui.disableAirconIntegration.disabled = state.airconBusy || !enabled;
        ui.airconIntegrationToken.value = enabled ? aircon.integration_token || "" : "";
        ui.copyAirconToken.disabled = !enabled || !ui.airconIntegrationToken.value;
        ui.rotateAirconToken.disabled = state.airconBusy || !enabled;
        if (!enabled) {
            ui.airconTriggerUrl.value = "";
            ui.airconUrlGuide.value = "";
            return;
        }
        const current = aircon.state || {};
        const parameters = current.power
            ? {
                operation: "set",
                temperature_c: String(current.temperature_c || 26),
                mode: current.mode || "cool",
                eco: String(!!current.eco),
            }
            : { operation: "off" };
        ui.airconTriggerUrl.value = airconTriggerUrl(parameters);
        ui.airconUrlGuide.value = airconUrlGuide();
    }

    async function sendAirconRemoteCommand(command, capability, label) {
        if (state.airconBusy || !state.airconAvailable) return;
        state.airconBusy = true;
        state.airconPendingVerification = null;
        renderAirconRemote(state.aircon?.state || {}, state.airconAvailable);
        try {
            const body = await api("/api/v1/aircon/commands", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ command, request_id: "" }),
            });
            const result = body.result || {};
            state.airconPendingVerification = { capability, label };
            ui.irLastResult.textContent = result.message || `${label}红外已发送`;
            showToast(`${label}红外已发送，请确认实体空调`);
            state.airconDirty = false;
        } catch (error) {
            ui.irLastResult.textContent = `操作失败：${error.message || error}`;
            handleError(error);
        } finally {
            state.airconBusy = false;
            await loadVoice(true);
        }
    }

    async function updateAirconVerification(verified) {
        const pending = state.airconPendingVerification;
        if (!pending || state.airconBusy) return;
        state.airconBusy = true;
        try {
            await api("/api/v1/aircon/verification", {
                method: "PUT",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ capability: pending.capability, verified }),
            });
            showToast(verified
                ? `${pending.label}已记为通过`
                : `${pending.label}未通过，外部接口保持关闭`);
            state.airconPendingVerification = null;
        } catch (error) {
            handleError(error);
        } finally {
            state.airconBusy = false;
            await loadVoice(true);
        }
    }

    async function resetAirconVerification(capability) {
        if (state.airconBusy) return;
        state.airconBusy = true;
        try {
            await api("/api/v1/aircon/verification", {
                method: "PUT",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ capability, verified: false }),
            });
            showToast("该项已重置，外部接口已关闭");
        } catch (error) {
            handleError(error);
        } finally {
            state.airconBusy = false;
            await loadVoice(true);
        }
    }

    async function setAirconIntegration(enabled) {
        if (state.airconBusy) return;
        const prompt = enabled
            ? "启用后将生成全新 Token，外部 AI 可通过 URL 控制空调。继续？"
            : "关闭后现有空调 Token 和触发 URL 将立即停止工作。继续？";
        if (!window.confirm(prompt)) return;
        state.airconBusy = true;
        try {
            await api("/api/v1/aircon/integration", {
                method: "PUT",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ enabled }),
            });
            showToast(enabled ? "外部空调接口已启用" : "外部空调接口已关闭");
        } catch (error) {
            handleError(error);
        } finally {
            state.airconBusy = false;
            await loadVoice(true);
        }
    }

    async function submitAirconCommand(event) {
        event.preventDefault();
        if (state.airconBusy || !ui.airconCommandForm.reportValidity()) return;
        const payload = airconCommandPayload();
        const summary = formatAirconOperation(payload.command);
        if (!window.confirm(`执行“${summary}”？`)) return;
        state.airconBusy = true;
        ui.sendAirconCommand.disabled = true;
        try {
            const body = await api("/api/v1/aircon/commands", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(payload),
            });
            const result = body.result || {};
            ui.irLastResult.textContent = result.message || `${summary}已提交`;
            showToast(result.message || `${summary}已提交`);
            state.airconDirty = false;
        } catch (error) {
            ui.irLastResult.textContent = `操作失败：${error.message || error}`;
            handleError(error);
        } finally {
            state.airconBusy = false;
            await loadVoice(true);
        }
    }

    async function submitAirconTimer(event) {
        event.preventDefault();
        if (state.airconBusy || !ui.airconTimerPanel.reportValidity()) return;
        const hours = Math.floor(Number(ui.airconTimerHours.value || 0));
        const minutes = Math.floor(Number(ui.airconTimerMinutes.value || 0));
        const delayMinutes = hours * 60 + minutes;
        if (delayMinutes < 1 || delayMinutes > 43200) {
            showToast("定时时长需在 1 分钟至 30 天之间");
            return;
        }
        const current = state.aircon?.state || {};
        const turningOn = ui.airconTimerOperation.value === "set";
        const command = turningOn
            ? {
                operation: "set",
                temperature_c: Number(current.temperature_c) || 26,
                mode: current.mode || "cool",
                eco: !!current.eco && current.mode === "cool",
            }
            : { operation: "off" };
        const duration = [
            hours ? `${hours} 小时` : "",
            minutes ? `${minutes} 分钟` : "",
        ].filter(Boolean).join(" ");
        const summary = turningOn
            ? `${duration}后按当前状态开机`
            : `${duration}后关闭空调`;
        if (!window.confirm(`创建“${summary}”定时任务？`)) return;
        state.airconBusy = true;
        try {
            const body = await api("/api/v1/aircon/commands", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({
                    command,
                    delay_minutes: delayMinutes,
                    request_id: `web-timer-${Date.now()}`,
                }),
            });
            ui.airconTimerPanel.hidden = true;
            ui.airconTimerButton.setAttribute("aria-expanded", "false");
            showToast(body.result?.message || "定时任务已创建");
        } catch (error) {
            handleError(error);
        } finally {
            state.airconBusy = false;
            await loadVoice(true);
        }
    }

    async function cancelAirconSchedule(scheduleId) {
        if (state.airconBusy) return;
        state.airconBusy = true;
        try {
            const command = scheduleId
                ? { operation: "cancel_schedule", schedule_id: scheduleId }
                : { operation: "cancel_all" };
            const body = await api("/api/v1/aircon/commands", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ command, request_id: "" }),
            });
            showToast(body.result?.message || "定时任务已取消");
        } catch (error) {
            handleError(error);
        } finally {
            state.airconBusy = false;
            await loadVoice(true);
        }
    }

    async function rotateAirconToken() {
        if (state.airconBusy) return;
        if (!window.confirm("重新生成后，外部 AI 助手使用的旧 Token 会立即失效。继续？")) return;
        state.airconBusy = true;
        try {
            await api("/api/v1/aircon/integration-token", { method: "POST" });
            showToast("空调集成 Token 已更新");
        } catch (error) {
            handleError(error);
        } finally {
            state.airconBusy = false;
            await loadVoice(true);
        }
    }

    async function copyAirconValue(input, message) {
        if (!input.value) return;
        try {
            await navigator.clipboard.writeText(input.value);
        } catch (_) {
            input.select();
            document.execCommand("copy");
        }
        showToast(message);
    }

    async function controlIrService(action) {
        if (state.irBusy) return;
        state.irBusy = true;
        ui.irServiceControls.querySelectorAll("button, input").forEach((control) => {
            control.disabled = true;
        });
        try {
            await api(`/api/v1/components/ir/${action}`, { method: "POST" });
            const label = { start: "启动", restart: "重启", stop: "停止" }[action];
            showToast(`红外服务已${label}`);
        } catch (error) {
            handleError(error);
        } finally {
            state.irBusy = false;
            await loadVoice(true);
        }
    }

    async function setIrAutostart(enabled) {
        if (state.irBusy) return;
        state.irBusy = true;
        try {
            await api("/api/v1/components/ir/autostart", {
                method: "PUT",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ enabled }),
            });
            showToast("红外服务开机自启已更新");
        } catch (error) {
            handleError(error);
        } finally {
            state.irBusy = false;
            await loadVoice(true);
        }
    }

    function renderVoiceAssets(assets) {
        state.voiceAssets = cloneJson(assets);
        state.voiceAssetsInstalling = !!assets.busy;
        const entries = [
            ["voice-tts", assets.voice_tts || {}],
            ["voice-kws", assets.voice_kws || {}],
            ["voice-asr", assets.voice_asr || {}],
        ];
        const stateLabels = {
            queued: "等待下载",
            downloading: "正在下载",
            verifying: "正在校验",
            installing: "正在安装",
            reinitializing: "正在初始化服务",
            installed: "已安装",
            missing: "未安装",
            failed: "安装失败",
            unsupported: "当前版本不支持",
        };
        for (const [id, asset] of entries) {
            const container = ui.voiceServiceControls
                .querySelector(`[data-voice-asset="${id}"]`);
            if (!container) continue;
            const detail = container.querySelector("[data-asset-detail]");
            const progress = container.querySelector("[data-asset-progress]");
            const install = container.querySelector("[data-asset-install]");
            const active = !!asset.active;
            const phase = stateLabels[asset.state] || asset.state || "状态未知";
            if (!asset.supported) {
                detail.textContent = "当前 binary 未包含 voice-workers 功能";
            } else if (asset.state === "failed") {
                detail.textContent = asset.last_error || asset.detail || phase;
            } else if (active) {
                detail.textContent =
                    `${phase} · ${formatBytes(asset.downloaded_bytes)} / ` +
                    `${formatBytes(asset.download_bytes)}`;
            } else if (asset.installed) {
                detail.textContent =
                    `${asset.version || "当前版本"} · ${formatBytes(asset.installed_bytes)}`;
            } else {
                detail.textContent =
                    `下载 ${formatBytes(asset.download_bytes)} · ` +
                    `安装需约 ${formatBytes(asset.required_available_bytes)} 可用空间`;
            }
            progress.hidden = !active;
            progress.value = Number(asset.progress_percent || 0);
            install.textContent = asset.installed ? "重新安装模型" : "安装模型";
            install.disabled = !asset.installable;
        }
    }

    async function loadVoiceAssets(silent = true) {
        if (state.voiceAssetsBusy || state.voiceBusy) return;
        state.voiceAssetsBusy = true;
        let installationFinished = false;
        try {
            const body = await api("/api/v1/assets");
            const wasInstalling = state.voiceAssetsInstalling;
            renderVoiceAssets(body && body.assets || {});
            installationFinished = wasInstalling && !state.voiceAssetsInstalling;
        } catch (error) {
            if (!silent) handleError(error);
        } finally {
            state.voiceAssetsBusy = false;
        }
        if (installationFinished) await loadVoice(true);
    }

    async function installVoiceAsset(assetId) {
        if (state.voiceAssetsBusy || state.voiceAssetsInstalling) return;
        const key = assetId === "voice-tts" ? "voice_tts" : "voice_kws";
        const asset = state.voiceAssets && state.voiceAssets[key] || {};
        const verb = asset.installed ? "重新安装" : "安装";
        const message =
            `${verb}${asset.label || "语音模型"}？需要下载 ` +
            `${formatBytes(asset.download_bytes)}，安装期间对应服务会短暂停止。`;
        if (!window.confirm(message)) return;
        state.voiceAssetsBusy = true;
        ui.voiceServiceControls.querySelectorAll("[data-asset-install]").forEach((button) => {
            button.disabled = true;
        });
        try {
            await api(`/api/v1/assets/${assetId}/install`, { method: "POST" });
            showToast(`${asset.label || "语音模型"}已开始安装`);
            await loadVoice(true);
        } catch (error) {
            handleError(error);
        } finally {
            state.voiceAssetsBusy = false;
            await loadVoiceAssets(true);
        }
    }

    function renderManagedVoiceService(id, service, controlAvailable) {
        const stateElement = id === "tts" ? ui.ttsServiceState : ui.voiceServiceState;
        const detailElement = id === "tts" ? ui.ttsServiceDetail : ui.voiceServiceDetail;
        const pidElement = id === "tts" ? ui.ttsServicePid : ui.voiceServicePid;
        const labels = {
            ready: "正常",
            idle: "空闲",
            busy: "忙碌",
            listening: "监听中",
            disabled: "监听关闭",
            starting: "启动中",
            stopped: "已停止",
            unavailable: "未安装",
            unhealthy: "异常",
            "config-error": "配置错误",
            "audio-error": "音频异常",
            failed: "启动失败",
        };
        stateElement.textContent = labels[service.state] || service.state || "未知";
        stateElement.className =
            `chip ${service.healthy ? "active" : service.running ? "" : "offline"}`.trim();
        detailElement.textContent = service.detail || "--";
        pidElement.textContent = service.pid ? String(service.pid) : "--";
        const autostart = ui.voiceServiceControls
            .querySelector(`[data-service-autostart="${id}"]`);
        autostart.checked = !!service.autostart;
        autostart.disabled =
            state.voiceServicesBusy || state.voiceAssetsInstalling || !controlAvailable;
        ui.voiceServiceControls
            .querySelectorAll(`[data-service-control="${id}"]`)
            .forEach((button) => {
                const action = button.dataset.serviceAction;
                button.disabled = state.voiceServicesBusy ||
                    state.voiceAssetsInstalling ||
                    !controlAvailable ||
                    !service.installed ||
                    (action === "start" ? service.running : !service.running);
            });
    }

    async function controlVoiceService(service, action) {
        if (state.voiceServicesBusy) return;
        if (action === "stop") {
            const label = service === "tts" ? "TTS 声纹合成" : "语音识别";
            const suffix = service === "tts"
                ? "停止后本机回复会回退系统声音，公共语音也将不可用。"
                : "停止后将不再监听语音命令。";
            if (!window.confirm(`确认停止${label}？${suffix}`)) return;
        }
        state.voiceServicesBusy = true;
        ui.voiceServiceControls.querySelectorAll("button, input").forEach((control) => {
            control.disabled = true;
        });
        try {
            const body = await api(`/api/v1/components/${service}/${action}`, {
                method: "POST",
            });
            renderVoiceServices(body.components || {});
            const actionLabel = { start: "启动", stop: "停止", restart: "重启" }[action];
            showToast(`${service === "tts" ? "TTS" : "语音识别"}已${actionLabel}`);
        } catch (error) {
            handleError(error);
        } finally {
            state.voiceServicesBusy = false;
            await loadVoice(true);
        }
    }

    async function setVoiceServiceAutostart(service, enabled) {
        if (state.voiceServicesBusy) return;
        state.voiceServicesBusy = true;
        ui.voiceServiceControls.querySelectorAll("button, input").forEach((control) => {
            control.disabled = true;
        });
        try {
            const body = await api(`/api/v1/components/${service}/autostart`, {
                method: "PUT",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ enabled }),
            });
            renderVoiceServices(body.components || {});
            showToast(`${service === "tts" ? "TTS" : "语音识别"}开机自启已更新`);
        } catch (error) {
            handleError(error);
        } finally {
            state.voiceServicesBusy = false;
            await loadVoice(true);
        }
    }

    function voiceCommandPhrases(command) {
        const phrases = Array.isArray(command.phrases)
            ? command.phrases.map((phrase) => String(phrase || "").trim()).filter(Boolean)
            : [];
        if (!phrases.length && command.phrase) phrases.push(command.phrase);
        return phrases.length ? phrases : [""];
    }

    function voicePhraseRow(phrase, removable) {
        return `
            <div class="voice-phrase-row">
                <input data-voice-phrase type="text" maxlength="24"
                       value="${esc(phrase)}" required>
                <button class="button ghost" type="button"
                        data-voice-action="remove-phrase"
                        ${removable ? "" : "disabled"}>移除</button>
            </div>`;
    }

    function syncVoicePhraseButtons(editor) {
        const rows = editor.querySelectorAll(".voice-phrase-row");
        rows.forEach((row) => {
            row.querySelector('[data-voice-action="remove-phrase"]').disabled =
                rows.length === 1;
        });
        editor.querySelector('[data-voice-action="add-phrase"]').disabled =
            rows.length >= 8;
    }

    function renderVoiceCommands(commands) {
        if (!commands.length) {
            ui.voiceCommandList.innerHTML = '<div class="empty">暂无语音命令</div>';
            return;
        }
        ui.voiceCommandList.innerHTML = commands.map((command, index) => {
            const phrases = voiceCommandPhrases(command);
            return `
                <article class="voice-command" data-command-index="${index}">
                <header>
                    <label class="toggle">
                        <input type="checkbox" data-voice-field="enabled"
                               ${command.enabled ? "checked" : ""}>
                        <span>${esc(phrases[0] || "未命名命令")}</span>
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
                    <div class="voice-phrases-field">
                        <span>触发短语</span>
                        <div class="voice-phrase-editor" data-voice-field="phrases">
                            <div class="voice-phrase-list">
                                ${phrases.map((phrase) =>
                                    voicePhraseRow(phrase, phrases.length > 1)).join("")}
                            </div>
                            <button class="button ghost voice-add-phrase" type="button"
                                    data-voice-action="add-phrase"
                                    ${phrases.length >= 8 ? "disabled" : ""}>添加说法</button>
                        </div>
                    </div>
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
                               min="0.01" max="0.95" step="0.01"
                               value="${Number(command.trigger_threshold ?? 0.05).toFixed(2)}" required></label>
                    <label><span>冷却（毫秒）</span>
                        <input data-voice-field="cooldown_ms" type="number"
                               min="500" max="60000" step="100"
                               value="${Number(command.cooldown_ms || 500)}" required></label>
                </div>
                <input data-voice-field="id" type="hidden" value="${esc(command.id)}">
            </article>`;
        }).join("");
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
        const config = cloneJson(state.voiceConfig || {});
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
            const phrases = Array.from(row.querySelectorAll("[data-voice-phrase]"))
                .map((input) => input.value.trim())
                .filter(Boolean);
            return {
                id: field("id").value,
                enabled: field("enabled").checked,
                phrase: phrases[0] || "",
                phrases,
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
            state.voiceConfig = cloneJson(body.config);
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
            phrases: ["小雨"],
            reply: "好的",
            method: "GET",
            url: "",
            body: "",
            boosting_score: 1.5,
            trigger_threshold: 0.05,
            cooldown_ms: 500,
        });
        state.voiceDirty = true;
        renderVoiceCommands(commands);
        ui.voiceCommandList.lastElementChild?.querySelector(
            "[data-voice-phrase]",
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

    async function requestVoiceTranscription() {
        if (state.transcribeBusy) return;
        const windowMs = Number(ui.voiceTranscribeWindow?.value) || 6000;
        state.transcribeBusy = true;
        if (ui.voiceTranscribeButton) {
            ui.voiceTranscribeButton.disabled = true;
            ui.voiceTranscribeButton.textContent = "识别中…";
        }
        try {
            await api("/api/v1/voice/transcribe", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ window_ms: windowMs }),
            });
            showToast("已开始录音，请说话");
            const deadline = Date.now() + windowMs + 8000;
            const poll = async () => {
                await loadVoice(true);
                if (Date.now() < deadline) {
                    window.setTimeout(poll, 1200);
                } else {
                    state.transcribeBusy = false;
                    await loadVoice(true);
                }
            };
            window.setTimeout(poll, 1200);
        } catch (error) {
            state.transcribeBusy = false;
            handleError(error);
            await loadVoice(true);
        }
    }

    async function startVoiceReferenceRecording() {
        if (state.voiceProfileBusy) return;
        if (state.voiceDirty) {
            showToast("请先保存语音配置", true);
            return;
        }
        if (!navigator.mediaDevices?.getUserMedia || typeof MediaRecorder !== "function") {
            showToast("当前浏览器不支持录音", true);
            return;
        }
        setVoiceProfileBusy(true);
        let stream = null;
        try {
            stream = await navigator.mediaDevices.getUserMedia({
                audio: {
                    channelCount: 1,
                    echoCancellation: false,
                    noiseSuppression: false,
                    autoGainControl: false,
                },
            });
            const recorder = new MediaRecorder(stream);
            const chunks = [];
            state.voiceStream = stream;
            state.voiceRecorder = recorder;
            state.voiceChunks = chunks;
            recorder.addEventListener("dataavailable", (event) => {
                if (event.data.size) chunks.push(event.data);
            });
            recorder.addEventListener(
                "stop",
                () => uploadVoiceReference(recorder, stream, chunks),
                { once: true },
            );
            recorder.start(250);
            ui.stopVoiceReference.disabled = false;
            ui.voiceReferenceStatus.textContent = "正在录音，请完整朗读上方文稿";
            clearTimeout(state.voiceRecordTimer);
            state.voiceRecordTimer = window.setTimeout(stopVoiceReferenceRecording, 15_000);
        } catch (error) {
            stopVoiceReferenceTracks(stream);
            setVoiceProfileBusy(false);
            handleError(error);
        }
    }

    function stopVoiceReferenceRecording() {
        clearTimeout(state.voiceRecordTimer);
        state.voiceRecordTimer = 0;
        if (state.voiceRecorder?.state === "recording") {
            state.voiceRecorder.stop();
        }
        ui.stopVoiceReference.disabled = true;
    }

    function stopVoiceReferenceTracks(stream = state.voiceStream) {
        stream?.getTracks().forEach((track) => track.stop());
        if (state.voiceStream === stream) state.voiceStream = null;
    }

    function setVoiceProfileBusy(busy) {
        state.voiceProfileBusy = busy;
        ui.voiceForm.querySelectorAll("input, select, textarea, button").forEach((control) => {
            control.disabled = busy;
        });
        ui.voiceProfilePanel.querySelectorAll("button").forEach((control) => {
            control.disabled = busy;
        });
        ui.reloadVoice.disabled = busy;
        if (busy) return;
        ui.stopVoiceReference.disabled = true;
        ui.deleteVoiceReference.disabled =
            Number(state.voiceConfig?.voice_profile_revision || 0) === 0;
    }

    async function uploadVoiceReference(recorder, stream, chunks) {
        stopVoiceReferenceTracks(stream);
        ui.voiceReferenceStatus.textContent = "正在处理并录入声纹";
        try {
            const blob = new Blob(chunks, {
                type: recorder.mimeType || "audio/webm",
            });
            const wav = await audioBlobToWav(blob, 24_000);
            if (state.voicePreviewUrl) URL.revokeObjectURL(state.voicePreviewUrl);
            state.voicePreviewUrl = URL.createObjectURL(wav);
            ui.voiceReferencePreview.src = state.voicePreviewUrl;
            ui.voiceReferencePreview.hidden = false;
            const response = await api("/api/v1/voice/reference", {
                method: "PUT",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ audio_base64: await blobToBase64(wav) }),
            });
            state.voiceConfig = cloneJson(response.config);
            state.voiceDirty = false;
            showToast("声纹已录入，正在重新生成全部回复");
        } catch (error) {
            handleError(error);
        } finally {
            if (state.voiceRecorder === recorder) {
                state.voiceRecorder = null;
                state.voiceChunks = [];
            }
            setVoiceProfileBusy(false);
            await loadVoice(true);
        }
    }

    async function deleteVoiceReference() {
        if (state.voiceProfileBusy) return;
        if (state.voiceDirty) {
            showToast("请先保存或重新加载语音配置", true);
            return;
        }
        if (!window.confirm("删除声纹并恢复系统声音？")) return;
        setVoiceProfileBusy(true);
        try {
            const response = await api("/api/v1/voice/reference", { method: "DELETE" });
            state.voiceConfig = cloneJson(response.config);
            state.voiceDirty = false;
            if (state.voicePreviewUrl) URL.revokeObjectURL(state.voicePreviewUrl);
            state.voicePreviewUrl = "";
            ui.voiceReferencePreview.removeAttribute("src");
            ui.voiceReferencePreview.hidden = true;
            showToast("已恢复系统声音");
        } catch (error) {
            handleError(error);
        } finally {
            setVoiceProfileBusy(false);
            await loadVoice(true);
        }
    }

    async function audioBlobToWav(blob, sampleRate) {
        const context = new AudioContext();
        try {
            const decoded = await context.decodeAudioData(await blob.arrayBuffer());
            const frameCount = Math.max(1, Math.round(decoded.duration * sampleRate));
            const offline = new OfflineAudioContext(1, frameCount, sampleRate);
            const source = offline.createBufferSource();
            source.buffer = decoded;
            source.connect(offline.destination);
            source.start();
            const rendered = await offline.startRendering();
            return encodePcmWav(rendered.getChannelData(0), sampleRate);
        } finally {
            await context.close();
        }
    }

    function encodePcmWav(samples, sampleRate) {
        const buffer = new ArrayBuffer(44 + samples.length * 2);
        const view = new DataView(buffer);
        const writeText = (offset, text) => {
            for (let index = 0; index < text.length; index += 1) {
                view.setUint8(offset + index, text.charCodeAt(index));
            }
        };
        writeText(0, "RIFF");
        view.setUint32(4, 36 + samples.length * 2, true);
        writeText(8, "WAVE");
        writeText(12, "fmt ");
        view.setUint32(16, 16, true);
        view.setUint16(20, 1, true);
        view.setUint16(22, 1, true);
        view.setUint32(24, sampleRate, true);
        view.setUint32(28, sampleRate * 2, true);
        view.setUint16(32, 2, true);
        view.setUint16(34, 16, true);
        writeText(36, "data");
        view.setUint32(40, samples.length * 2, true);
        for (let index = 0; index < samples.length; index += 1) {
            const sample = Math.max(-1, Math.min(1, samples[index]));
            view.setInt16(44 + index * 2, sample < 0 ? sample * 32768 : sample * 32767, true);
        }
        return new Blob([buffer], { type: "audio/wav" });
    }

    async function blobToBase64(blob) {
        const bytes = new Uint8Array(await blob.arrayBuffer());
        let binary = "";
        for (let offset = 0; offset < bytes.length; offset += 0x8000) {
            binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
        }
        return btoa(binary);
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
        renderQqPushAccess(config, body.push_endpoint);
        renderQqMessages(
            Array.isArray(body.recent_messages) ? body.recent_messages : [],
            Array.isArray(config.groups) ? config.groups : [],
        );

        if (!state.qqDirty) {
            state.qqConfig = cloneJson(config);
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

    function renderQqPushAccess(config, pushEndpoint) {
        const token = config.push_token || "";
        const configured = !!config.push_token_configured;
        ui.qqPushEndpoint.value =
            `${location.origin}${pushEndpoint || "/api/v1/integrations/qq/notify"}`;
        ui.qqPushToken.value = token;
        ui.qqPushTokenState.textContent = token
            ? "Token 已显示"
            : configured ? "重新生成后可显示" : "尚未生成 Token";
        ui.qqPushTokenState.className = `chip ${configured ? "active" : ""}`.trim();
        ui.rotateQqPushToken.textContent = configured ? "重新生成 Token" : "生成 Token";
        ui.copyQqPushToken.disabled = !token;
        const commandToken = token || "$CAMERA_HUB_QQ_PUSH_TOKEN";
        ui.qqCurlExample.textContent = [
            `curl -X POST '${ui.qqPushEndpoint.value}' \\`,
            `  -H "Authorization: Bearer ${commandToken}" \\`,
            "  -H 'Content-Type: application/json' \\",
            `  --data '{"target":"default","content":"任务执行完成"}'`,
        ].join("\n");
    }

    function renderQqMessages(messages, groups) {
        ui.qqMessageCount.textContent = `${messages.length} 条`;
        if (!messages.length) {
            ui.qqMessageList.innerHTML = '<div class="empty">暂无收到的消息</div>';
            return;
        }
        const groupNames = new Map(
            groups.map((group) => [group.openid, group.name || compactOpenid(group.openid)]),
        );
        ui.qqMessageList.innerHTML = messages.map((message) => {
            const groupMessage = message.scene === "group";
            const sourceOpenid = message.source_openid || "";
            const sourceName = groupMessage
                ? groupNames.get(sourceOpenid) || "QQ群"
                : message.sender_name || "QQ 用户";
            const sceneLabel = message.event_type === "GROUP_AT_MESSAGE_CREATE"
                ? "群聊 @"
                : groupMessage ? "群聊" : "单聊";
            const attachmentCount = Number(message.attachment_count || 0);
            const content = message.content
                || (attachmentCount ? `[${attachmentCount} 个附件]` : "[非文本消息]");
            const sender = message.sender_name
                ? `${message.sender_name} (${compactOpenid(message.sender_openid)})`
                : compactOpenid(message.sender_openid) || "未知发送者";
            const sourceTitle = sourceOpenid ? ` title="${esc(sourceOpenid)}"` : "";
            const attachment = attachmentCount
                ? `<span>${attachmentCount} 个附件</span>` : "";
            return `
                <article class="qq-message-row">
                    <header>
                        <div>
                            <span class="qq-message-scene">${sceneLabel}</span>
                            <strong${sourceTitle}>${esc(sourceName)}</strong>
                        </div>
                        <time>${formatTimestamp(
                            message.timestamp_epoch || message.received_epoch,
                        )}</time>
                    </header>
                    <p>${esc(content)}</p>
                    <footer>
                        <span title="${esc(message.sender_openid || "")}">${esc(sender)}</span>
                        ${attachment}
                    </footer>
                </article>`;
        }).join("");
    }

    function compactOpenid(value) {
        const text = String(value || "");
        return text.length > 20 ? `${text.slice(0, 8)}...${text.slice(-6)}` : text;
    }

    function renderQqGroups(groups, defaultGroup) {
        clearChildren(ui.qqDefaultGroup);
        clearChildren(ui.qqTestTarget);
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
            if (state.qqConfig) {
                state.qqConfig.push_token_configured = true;
                state.qqConfig.push_token = body.token || "";
            }
            await loadQq(true);
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

    async function copyQqCurlExample() {
        const command = ui.qqCurlExample.textContent || "";
        if (!command) return;
        try {
            await navigator.clipboard.writeText(command);
        } catch (_) {
            const selection = window.getSelection();
            const range = document.createRange();
            range.selectNodeContents(ui.qqCurlExample);
            selection.removeAllRanges();
            selection.addRange(range);
            document.execCommand("copy");
            selection.removeAllRanges();
        }
        showToast("cURL 示例已复制");
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

    async function loadDdns(silent = false) {
        if (state.ddnsBusy) return;
        state.ddnsBusy = true;
        try {
            const [body, componentsBody] = await Promise.all([
                api("/api/v1/ddns"),
                api("/api/v1/components"),
            ]);
            renderDdns(
                body && body.ddns || {},
                componentsBody && componentsBody.components || {},
            );
        } catch (error) {
            if (!silent) handleError(error);
        } finally {
            state.ddnsBusy = false;
        }
    }

    function renderDdns(body, components = {}) {
        const config = body.config || {};
        const status = body.status || {};
        const workerOnline = !!body.worker_online;
        state.ddnsWorkerOnline = workerOnline;
        const labels = {
            disabled: "已关闭",
            incomplete: "配置不完整",
            synchronizing: "同步中",
            online: "运行中",
            retrying: "正在重试",
            config_error: "配置错误",
            stopped: "已停止",
            not_started: "未启动",
        };
        ui.ddnsStatus.textContent = workerOnline
            ? labels[status.state] || status.state || "未知"
            : "进程离线";
        ui.ddnsStatus.className = `chip ${
            workerOnline && ["online", "synchronizing"].includes(status.state)
                ? "active" : "offline"
        }`;
        ui.ddnsState.textContent = workerOnline
            ? status.detail || status.state || "--"
            : "未收到 DDNS worker 心跳";
        ui.ddnsStableIpv6.textContent = status.stable_ipv6 || "--";
        ui.ddnsPrefix.textContent = status.prefix || "--";
        ui.ddnsLastSuccess.textContent = formatTimestamp(status.last_success_epoch);
        ui.ddnsNextAttempt.textContent = formatTimestamp(status.next_attempt_epoch);
        ui.ddnsChangedCount.textContent = `${Number(status.changed_count || 0)} 条`;
        ui.ddnsLastError.textContent = status.last_error || "--";
        ui.ddnsLastError.classList.toggle("error", !!status.last_error);
        ui.reconcileDdns.disabled = !workerOnline || !config.enabled || state.ddnsDirty;
        renderDdnsComponent(components.ddns || {}, !!components.control_available);

        if (!state.ddnsDirty) {
            state.ddnsConfig = cloneJson(config);
            ui.ddnsEnabled.checked = !!config.enabled;
            ui.ddnsDomain.value = config.domain || "";
            ui.ddnsSecretId.value = config.secret_id || "";
            ui.ddnsSecretKey.value = "";
            ui.ddnsInterface.value = config.interface || "wlan0";
            ui.ddnsTtl.value = config.ttl || 600;
            ui.ddnsInterval.value = config.interval_seconds || 60;
            ui.ddnsForceInterval.value = config.force_seconds || 21600;
            ui.ddnsSecretState.textContent = config.secret_key_configured
                ? "SecretKey 已配置，留空不会修改"
                : "尚未配置 SecretKey";
            ui.clearDdnsSecret.disabled = !config.secret_key_configured;
            renderDdnsRecords(Array.isArray(config.records) ? config.records : []);
        }
        updateDdnsCredentialRequirements();
    }

    function renderDdnsComponent(component, controlAvailable) {
        const labels = {
            running: "进程运行中",
            starting: "进程启动中",
            stopped: "进程已停止",
            unavailable: "组件不可用",
        };
        ui.ddnsProcessState.textContent =
            labels[component.state] || component.state || "进程状态未知";
        ui.ddnsProcessState.className =
            `chip ${component.running ? "active" : "offline"}`;
        ui.ddnsProcessPid.textContent = component.pid ? String(component.pid) : "--";
        const autostart = ui.ddnsServiceControls
            .querySelector('[data-service-autostart="ddns"]');
        autostart.checked = !!component.autostart;
        autostart.disabled = state.ddnsComponentBusy || !controlAvailable;
        ui.ddnsServiceControls.querySelectorAll("[data-service-control]").forEach((button) => {
            const action = button.dataset.serviceAction;
            button.disabled = state.ddnsComponentBusy ||
                !controlAvailable ||
                !component.installed ||
                (action === "start" ? component.running : !component.running);
        });
    }

    async function controlDdnsComponent(action) {
        if (state.ddnsComponentBusy) return;
        if (action === "stop" &&
            !window.confirm("确认停止 DDNS worker？停止后将不再自动对账域名。")) {
            return;
        }
        state.ddnsComponentBusy = true;
        ui.ddnsServiceControls.querySelectorAll("button, input").forEach((control) => {
            control.disabled = true;
        });
        try {
            await api(`/api/v1/components/ddns/${action}`, { method: "POST" });
            const actionLabel = { start: "启动", stop: "停止", restart: "重启" }[action];
            showToast(`DDNS worker 已${actionLabel}`);
        } catch (error) {
            handleError(error);
        } finally {
            state.ddnsComponentBusy = false;
            await loadDdns(true);
        }
    }

    async function setDdnsAutostart(enabled) {
        if (state.ddnsComponentBusy) return;
        state.ddnsComponentBusy = true;
        ui.ddnsServiceControls.querySelectorAll("button, input").forEach((control) => {
            control.disabled = true;
        });
        try {
            await api("/api/v1/components/ddns/autostart", {
                method: "PUT",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ enabled }),
            });
            showToast("DDNS worker 开机自启已更新");
        } catch (error) {
            handleError(error);
        } finally {
            state.ddnsComponentBusy = false;
            await loadDdns(true);
        }
    }

    function renderDdnsRecords(records) {
        if (!records.length) {
            ui.ddnsRecordList.innerHTML =
                '<div class="empty">尚未配置 AAAA 记录</div>';
            return;
        }
        ui.ddnsRecordList.innerHTML = records.map((record) => `
            <div class="ddns-record-row">
                <label>
                    <span>记录名</span>
                    <input data-ddns-field="name" type="text" maxlength="253"
                           value="${esc(record.name || "")}" placeholder="@ 或 mi6" required>
                </label>
                <label>
                    <span>IPv6 后 64 位</span>
                    <input data-ddns-field="iid" type="text" maxlength="39"
                           value="${esc(record.iid || "")}"
                           placeholder="528f:4cff:feef:dd90" required>
                </label>
                <button class="button danger" data-ddns-action="delete"
                        type="button">删除</button>
            </div>`).join("");
    }

    function addDdnsRecord(record = {}) {
        const empty = ui.ddnsRecordList.querySelector(".empty");
        if (empty) empty.remove();
        ui.ddnsRecordList.insertAdjacentHTML("beforeend", `
            <div class="ddns-record-row">
                <label>
                    <span>记录名</span>
                    <input data-ddns-field="name" type="text" maxlength="253"
                           value="${esc(record.name || "")}" placeholder="@ 或 mi6" required>
                </label>
                <label>
                    <span>IPv6 后 64 位</span>
                    <input data-ddns-field="iid" type="text" maxlength="39"
                           value="${esc(record.iid || "")}"
                           placeholder="528f:4cff:feef:dd90" required>
                </label>
                <button class="button danger" data-ddns-action="delete"
                        type="button">删除</button>
            </div>`);
        state.ddnsDirty = true;
        ui.reconcileDdns.disabled = true;
        ui.ddnsRecordList.lastElementChild
            ?.querySelector('[data-ddns-field="name"]')?.focus();
    }

    function collectDdnsRecords() {
        return Array.from(ui.ddnsRecordList.querySelectorAll(".ddns-record-row"))
            .map((row) => ({
                name: row.querySelector('[data-ddns-field="name"]').value.trim(),
                iid: row.querySelector('[data-ddns-field="iid"]').value.trim(),
            }));
    }

    function collectDdnsConfig(clearSecret = false) {
        return {
            revision: Number(state.ddnsConfig?.revision || 0),
            enabled: clearSecret ? false : ui.ddnsEnabled.checked,
            domain: ui.ddnsDomain.value.trim(),
            secret_id: ui.ddnsSecretId.value.trim(),
            secret_key: clearSecret ? "" : ui.ddnsSecretKey.value.trim(),
            clear_secret: clearSecret,
            interface: ui.ddnsInterface.value.trim(),
            ttl: Number(ui.ddnsTtl.value),
            interval_seconds: Number(ui.ddnsInterval.value),
            force_seconds: Number(ui.ddnsForceInterval.value),
            records: collectDdnsRecords(),
        };
    }

    function updateDdnsCredentialRequirements() {
        const configured = !!state.ddnsConfig?.secret_key_configured;
        const idChanged =
            ui.ddnsSecretId.value.trim() !== (state.ddnsConfig?.secret_id || "");
        ui.ddnsSecretId.required = ui.ddnsEnabled.checked;
        ui.ddnsSecretKey.required =
            ui.ddnsEnabled.checked && (!configured || idChanged);
    }

    async function saveDdnsConfig(event, clearSecret = false) {
        if (event) event.preventDefault();
        if (!clearSecret && !ui.ddnsForm.reportValidity()) return;
        ui.saveDdns.disabled = true;
        ui.clearDdnsSecret.disabled = true;
        try {
            await api("/api/v1/ddns", {
                method: "PUT",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(collectDdnsConfig(clearSecret)),
            });
            state.ddnsDirty = false;
            ui.ddnsSecretKey.value = "";
            showToast(clearSecret ? "DNSPod SecretKey 已清除" : "DDNS 配置已保存");
            await loadDdns(true);
        } catch (error) {
            handleError(error);
        } finally {
            ui.saveDdns.disabled = false;
            ui.clearDdnsSecret.disabled = !state.ddnsConfig?.secret_key_configured;
        }
    }

    async function previewDdns() {
        ui.previewDdns.disabled = true;
        try {
            const body = await api("/api/v1/ddns/preview", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({
                    domain: ui.ddnsDomain.value.trim(),
                    interface: ui.ddnsInterface.value.trim(),
                    records: collectDdnsRecords(),
                }),
            });
            const result = body.preview || {};
            const records = Array.isArray(result.records) ? result.records : [];
            ui.ddnsPreview.innerHTML = `
                <div class="ddns-preview-summary">
                    <span>稳定地址 <code>${esc(result.stable_ipv6 || "--")}</code></span>
                    <span>公网前缀 <code>${esc(result.prefix || "--")}</code></span>
                </div>
                <div class="ddns-preview-records">
                    ${records.map((record) => `
                        <div>
                            <strong>${esc(record.fqdn || record.name)}</strong>
                            <code>${esc(record.address)}</code>
                        </div>`).join("")}
                </div>`;
        } catch (error) {
            handleError(error);
        } finally {
            ui.previewDdns.disabled = false;
        }
    }

    async function reconcileDdns() {
        if (state.ddnsDirty) {
            showToast("请先保存 DDNS 配置", true);
            return;
        }
        if (!window.confirm("立即读取并对账 DNSPod AAAA 记录？")) return;
        ui.reconcileDdns.disabled = true;
        try {
            await api("/api/v1/ddns/reconcile", { method: "POST" });
            showToast("已请求 DDNS 立即对账");
            window.setTimeout(() => loadDdns(true), 1200);
        } catch (error) {
            handleError(error);
        } finally {
            ui.reconcileDdns.disabled =
                !state.ddnsWorkerOnline || !state.ddnsConfig?.enabled || state.ddnsDirty;
        }
    }

    function renderTv(tv) {
        state.tv = tv;
        const config = tv.config || {};
        const status = tv.status || {};
        const phases = {
            disabled: "守护关闭", pending: "等待检测", checking: "确认状态",
            online: config.enabled ? "服务正常" : "服务正常 · 守护关闭",
            recovering: "恢复中", waiting: "等待上线 / 重试",
            filesystem_readonly: "电视文件系统只读",
            capture_unavailable: "控制正常 · 无截图",
            capture_stalled: "截图停止更新",
        };
        tvUi.tvStatus.textContent = phases[status.phase] || "等待检测";
        tvUi.tvStatus.className = `chip ${status.service_online ? "active" : "offline"}`;
        tvUi.tvMessage.textContent = status.message || "等待检测";
        tvUi.tvLastChecked.textContent = formatTimestamp(status.last_checked_epoch);
        tvUi.tvLastRecovered.textContent = formatTimestamp(status.last_recovered_epoch);
        tvUi.tvRecoveryCount.textContent = status.recovery_count || 0;
        tvUi.tvNextRetry.textContent = config.enabled ? formatTimestamp(status.next_retry_epoch) : "—";
        tvUi.tvError.hidden = !status.last_error;
        tvUi.tvError.textContent = status.last_error || "";
        tvUi.checkTv.disabled = state.tvBusy || state.tvDirty || !config.host || status.phase === "recovering";
        tvUi.saveTv.disabled = state.tvBusy;
        tvUi.uploadTvApk.disabled = state.tvBusy;
        tvUi.tvApkStatus.textContent = tv.apk_bytes
            ? `已保存 ${formatBytes(tv.apk_bytes)}，服务恢复时使用`
            : "尚未上传；恢复时使用电视上已有 APK";
        tvUi.tvLinks.hidden = !tv.control_url;
        if (tv.control_url) {
            tvUi.tvControlLink.href = `${tv.control_url}/`;
            tvUi.tvGuideLink.href = tv.guide_url;
        }
        if (!state.tvDirty) {
            tvUi.tvEnabled.checked = !!config.enabled;
            tvUi.tvHost.value = config.host || "";
            tvUi.tvAdbPort.value = config.adb_port || 5555;
            tvUi.tvPollSeconds.value = config.poll_seconds || 15;
            tvUi.tvQuality.value = config.jpeg_quality || 25;
            tvUi.tvFrameInterval.value = config.frame_interval_ms || 300;
            tvUi.tvApiOrigin.value = config.api_origin || "";
        }
    }

    async function loadTv(silent = false) {
        if (state.tvBusy) return;
        try {
            const body = await api("/api/v1/tv");
            if (!state.tvBusy) renderTv(body.tv || {});
        } catch (error) {
            if (!silent) handleError(error);
        }
    }

    async function tvAction(action, success) {
        if (state.tvBusy) return;
        state.tvBusy = true;
        if (state.tv) renderTv(state.tv);
        try {
            await action();
            showToast(success);
        } catch (error) {
            handleError(error);
        } finally {
            state.tvBusy = false;
            if (state.tv) renderTv(state.tv);
            await loadTv(true);
        }
    }

    tvUi.tvForm.addEventListener("input", () => {
        state.tvDirty = true;
        tvUi.checkTv.disabled = true;
    });
    tvUi.tvForm.addEventListener("submit", (event) => {
        event.preventDefault();
        if (!tvUi.tvForm.reportValidity()) return;
        const config = {
            enabled: tvUi.tvEnabled.checked,
            host: tvUi.tvHost.value.trim(),
            adb_port: Number(tvUi.tvAdbPort.value),
            poll_seconds: Number(tvUi.tvPollSeconds.value),
            jpeg_quality: Number(tvUi.tvQuality.value),
            frame_interval_ms: Number(tvUi.tvFrameInterval.value),
            api_origin: tvUi.tvApiOrigin.value.trim(),
        };
        tvAction(async () => {
            const body = await api("/api/v1/tv", {
                method: "PUT", headers: { "Content-Type": "application/json" },
                body: JSON.stringify(config),
            });
            state.tvDirty = false;
            renderTv(body.tv || {});
        }, "电视守护设置已保存");
    });
    tvUi.reloadTv.addEventListener("click", () => loadTv());
    tvUi.checkTv.addEventListener("click", () => {
        if (state.tvDirty) return showToast("请先保存电视设置", true);
        tvAction(() => api("/api/v1/tv/check", { method: "POST" }), "已请求检查并恢复");
    });
    tvUi.tvApkForm.addEventListener("submit", (event) => {
        event.preventDefault();
        const file = tvUi.tvApkFile.files[0];
        if (!file) return;
        if (file.size > 16 * 1024 * 1024) return showToast("APK 最大为 16 MiB", true);
        tvAction(async () => {
            const body = await api("/api/v1/tv/apk", {
                method: "POST", headers: { "Content-Type": "application/vnd.android.package-archive" },
                body: file,
            });
            renderTv(body.tv || {});
            tvUi.tvApkForm.reset();
        }, "恢复用 APK 已保存");
    });
    tvUi.copyTvGuide.addEventListener("click", async () => {
        try {
            await navigator.clipboard.writeText(state.tv.guide_url);
            showToast("AI 指南地址已复制");
        } catch (error) {
            handleError(error);
        }
    });

    async function loadHttpProxy(silent = false) {
        if (state.httpProxyBusy) return;
        state.httpProxyBusy = true;
        try {
            const body = await api("/api/v1/http-proxy");
            renderHttpProxy(body && body.http_proxy || {});
        } catch (error) {
            if (!silent) handleError(error);
        } finally {
            state.httpProxyBusy = false;
            if (state.httpProxy) renderHttpProxy(state.httpProxy);
        }
    }

    function httpProxyEndpoint() {
        return `${location.origin}${
            state.httpProxy?.endpoint || "/api/v1/integrations/http-proxy/request"
        }`;
    }

    function httpProxyUrl(targetUrl) {
        const params = new URLSearchParams({
            token: state.httpProxy?.token || "HTTP_PROXY_TOKEN",
            url: targetUrl,
        });
        return `${httpProxyEndpoint()}?${params}`;
    }

    function httpProxyCurlGuide() {
        const exampleUrl = "https://www.baidu.com";
        const encodedUrl = encodeURIComponent(exampleUrl);
        const target = httpProxyUrl(exampleUrl);
        return [
            "HTTP 代理调用说明",
            "",
            "url 参数必须使用 URL 编码，再作为 Hub 接口的 url 查询参数。",
            `示例目标 URL：${exampleUrl}`,
            `编码后的 url：${encodedUrl}`,
            "Hub 收到请求后会解码 url，并使用相同 HTTP 方法访问目标地址。",
            "",
            "GET 示例",
            `curl '${target}'`,
            "",
            "POST JSON 示例",
            `curl -X POST '${target}' \\`,
            "  -H 'Authorization: Bearer TARGET_TOKEN' \\",
            "  -H 'Content-Type: application/json' \\",
            "  --data '{\"enabled\":true}'",
        ].join("\n");
    }

    function renderHttpProxy(proxy) {
        state.httpProxy = cloneJson(proxy);
        const enabled = !!proxy.enabled;
        ui.httpProxyStatus.textContent = enabled ? "已启用" : "已关闭";
        ui.httpProxyStatus.className = `chip ${enabled ? "active" : "offline"}`;
        ui.httpProxyAccess.hidden = !enabled;
        ui.httpProxyToken.value = enabled ? proxy.token || "" : "";
        ui.httpProxyTemplate.value = enabled ? httpProxyCurlGuide() : "";
        ui.copyHttpProxyToken.disabled = !enabled || !ui.httpProxyToken.value;
        ui.copyHttpProxyTemplate.disabled = !enabled || !ui.httpProxyTemplate.value;
        ui.rotateHttpProxyToken.disabled = state.httpProxyBusy || !enabled;
        ui.testHttpProxy.disabled = state.httpProxyBusy || !enabled;
        if (!state.httpProxyDirty) {
            ui.httpProxyEnabled.checked = enabled;
        }
        updateHttpProxyTestForm();
    }

    async function saveHttpProxy(event) {
        event.preventDefault();
        if (!ui.httpProxyForm.reportValidity()) return;
        const enabled = ui.httpProxyEnabled.checked;
        if (!enabled && state.httpProxy?.enabled &&
            !window.confirm("关闭后，现有 HTTP 代理 Token 会立即停止工作。继续？")) {
            return;
        }
        state.httpProxyBusy = true;
        ui.saveHttpProxy.disabled = true;
        try {
            const body = await api("/api/v1/http-proxy", {
                method: "PUT",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ enabled }),
            });
            state.httpProxyDirty = false;
            renderHttpProxy(body.http_proxy || {});
            showToast(enabled ? "HTTP 代理配置已启用" : "HTTP 代理配置已保存");
        } catch (error) {
            handleError(error);
        } finally {
            state.httpProxyBusy = false;
            ui.saveHttpProxy.disabled = false;
            renderHttpProxy(state.httpProxy || {});
        }
    }

    async function rotateHttpProxyToken() {
        if (!window.confirm("重新生成后，旧 HTTP 代理 Token 会立即失效。继续？")) return;
        state.httpProxyBusy = true;
        try {
            const body = await api("/api/v1/http-proxy/token", { method: "POST" });
            if (state.httpProxy) state.httpProxy.token = body.token || "";
            renderHttpProxy(state.httpProxy || {});
            showToast("HTTP 代理 Token 已更新");
        } catch (error) {
            handleError(error);
        } finally {
            state.httpProxyBusy = false;
            renderHttpProxy(state.httpProxy || {});
        }
    }

    function updateHttpProxyTestForm() {
        const method = ui.httpProxyTestMethod.value;
        const bodyAllowed = !["GET", "HEAD"].includes(method);
        ui.httpProxyTestBody.disabled = !bodyAllowed;
        ui.testHttpProxy.textContent = `发起 ${method}`;
    }

    function httpProxyTestHeaders() {
        const value = ui.httpProxyTestHeaders.value.trim();
        if (!value) return {};
        const parsed = JSON.parse(value);
        if (!parsed || Array.isArray(parsed) || typeof parsed !== "object") {
            throw new Error("请求头必须是 JSON 对象");
        }
        const headers = {};
        for (const [name, headerValue] of Object.entries(parsed)) {
            if (typeof headerValue !== "string") {
                throw new Error(`请求头 ${name} 的值必须是字符串`);
            }
            headers[name] = headerValue;
        }
        return headers;
    }

    async function testHttpProxy(event) {
        event.preventDefault();
        if (!ui.httpProxyTestForm.reportValidity() || !state.httpProxy?.enabled) return;
        const method = ui.httpProxyTestMethod.value;
        state.httpProxyBusy = true;
        ui.testHttpProxy.disabled = true;
        ui.httpProxyTestResult.textContent = "请求中…";
        try {
            const options = {
                method,
                headers: httpProxyTestHeaders(),
                cache: "no-store",
            };
            if (!["GET", "HEAD"].includes(method) && ui.httpProxyTestBody.value) {
                options.body = ui.httpProxyTestBody.value;
            }
            const response = await fetch(
                httpProxyUrl(ui.httpProxyTestUrl.value.trim()),
                options,
            );
            const body = await response.text();
            const contentType = response.headers.get("Content-Type") || "未知类型";
            ui.httpProxyTestResult.textContent = [
                `${method} · HTTP ${response.status}`,
                contentType,
                "",
                body.slice(0, 10_000),
                body.length > 10_000 ? "\n[页面仅显示前 10000 个字符]" : "",
            ].join("\n");
            showToast(`代理请求完成：HTTP ${response.status}`);
        } catch (error) {
            ui.httpProxyTestResult.textContent = `请求失败：${error.message || error}`;
            handleError(error);
        } finally {
            state.httpProxyBusy = false;
            ui.testHttpProxy.disabled = !state.httpProxy?.enabled;
        }
    }

    function renderDeviceSelect() {
        for (const select of [
            ui.deviceSelect,
            ui.liveDeviceSelect,
            ui.photoDeviceSelect,
        ]) {
            clearChildren(select);
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
        stopHubLive();
        stopRecordPlayback();
        state.device = deviceId;
        state.date = "";
        state.photoSelection.clear();
        daysRequest += 1;
        recordsRequest += 1;
        photosRequest += 1;
        ui.refreshPhotos.disabled = false;
        state.photos = [];
        renderPhotos();
        renderRecords([]);
        renderDevices();
        renderDeviceSelect();
        await loadDays();
        if (deviceId !== state.device) return;
        if (state.view === "photos") await loadPhotos();
    }

    let daysRequest = 0;
    let recordsRequest = 0;
    let photosRequest = 0;

    async function loadDays() {
        const request = ++daysRequest;
        const deviceId = state.device;
        clearChildren(ui.dateSelect);
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
                `/api/v1/devices/${encodeURIComponent(deviceId)}/records/days`,
            );
            if (request !== daysRequest || deviceId !== state.device) return;
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
            if (request !== daysRequest || deviceId !== state.device) return;
            handleError(error);
        }
    }

    async function loadRecords() {
        const request = ++recordsRequest;
        const deviceId = state.device;
        const date = state.date;
        if (!state.device || !state.date) {
            renderRecords([]);
            return;
        }
        ui.summary.textContent = "正在读取远端录像目录…";
        try {
            const body = await api(
                `/api/v1/devices/${encodeURIComponent(deviceId)}` +
                `/records/${encodeURIComponent(date)}`,
            );
            if (request !== recordsRequest || deviceId !== state.device || date !== state.date) return;
            renderRecords(Array.isArray(body.records) ? body.records : []);
        } catch (error) {
            if (request !== recordsRequest || deviceId !== state.device || date !== state.date) return;
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
        const request = ++photosRequest;
        const deviceId = state.device;
        if (!state.device) {
            state.photos = [];
            renderPhotos();
            return;
        }
        ui.refreshPhotos.disabled = true;
        ui.photoSummary.textContent = "正在读取节点 AI 抓拍…";
        try {
            const body = await api(
                `/api/v1/devices/${encodeURIComponent(deviceId)}/photos`,
            );
            if (request !== photosRequest || deviceId !== state.device) return;
            state.photos = Array.isArray(body.photos) ? body.photos : [];
            const available = new Set(state.photos.map((photo) => photo.name));
            for (const name of state.photoSelection) {
                if (!available.has(name)) state.photoSelection.delete(name);
            }
            renderPhotos();
        } catch (error) {
            if (request !== photosRequest || deviceId !== state.device) return;
            state.photos = [];
            renderPhotos();
            handleError(error);
        } finally {
            if (request === photosRequest && deviceId === state.device) ui.refreshPhotos.disabled = false;
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
            const opened = await waitForSourceOpen(playbackSource);
            if (epoch !== playbackEpoch) return;
            if (!opened) {
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
        stopHubLive();
        const session = liveSession;
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
            if (session !== liveSession) return;
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
        stopHubLive();
        const session = liveSession;
        if (!state.device) {
            setLiveStatus("暂无在线设备");
            return;
        }
        if (!window.CameraHubFlv) {
            setLiveStatus("HTTP-FLV 播放组件尚未加载");
            return;
        }
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
        stopHubLive();
        const session = liveSession;
        if (!state.device) {
            setLiveStatus("暂无在线设备");
            return;
        }
        if (typeof RTCPeerConnection !== "function") {
            setLiveStatus("当前浏览器不支持 WebRTC");
            return;
        }
        const deviceId = state.device;
        const sessionId = newWebRtcSessionId();
        liveWebRtcSession = { deviceId, sessionId };
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
            if (peer !== livePeer || session !== liveSession) return;
            await peer.setLocalDescription(offer);
            await waitForIceGathering(peer);
            if (peer !== livePeer || session !== liveSession) return;
            const response = await fetch(
                `/api/v1/devices/${encodeURIComponent(deviceId)}/webrtc/offer`,
                {
                    method: "POST",
                    cache: "no-store",
                    headers: { "Content-Type": "application/json" },
                    body: JSON.stringify({ sdp: peer.localDescription.sdp, session_id: sessionId }),
                },
            );
            if (!response.ok) throw new Error(await response.text());
            const body = await response.json();
            if (peer !== livePeer || session !== liveSession) {
                closeWebRtcSession(deviceId, sessionId);
                return;
            }
            await peer.setRemoteDescription({ type: "answer", sdp: body.sdp });
            if (peer !== livePeer || session !== liveSession) return;
            setLiveStatus("WebRTC ICE/DTLS 连接中…");
        } catch (error) {
            try { peer.close(); } catch (_) {}
            closeWebRtcSession(deviceId, sessionId);
            if (session !== liveSession) return;
            if (livePeer === peer) livePeer = null;
            liveWebRtcSession = null;
            liveMode = "";
            setLiveButtons();
            setLiveStatus(`WebRTC 协商失败：${error.message || error}`);
        }
    }

    async function startHubMoq() {
        stopHubLive();
        const session = liveSession;
        const deviceId = state.device;
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
        liveMode = "moq";
        setLiveButtons(liveMode);
        setLiveStatus("正在检查 MoQ 服务…");
        try {
            const body = await api("/api/v1/moq/status");
            if (session !== liveSession || deviceId !== state.device) return;
            const moq = body && body.moq || {};
            if (!moq.enabled || !moq.running) {
                throw new Error(moq.last_error || "MoQ 服务未启动");
            }
            let host = location.hostname;
            if (host.includes(":") && !host.startsWith("[")) host = `[${host}]`;
            liveMode = "moq";
            setLiveButtons(liveMode);
            ui.livePlayer.hidden = true;
            ui.moqPlayer.hidden = false;
            setLiveStatus("正在建立 MoQ/WebTransport 会话…");
            window.CameraHubMoq.start(
                ui.moqPlayer,
                {
                    url: `https://${host}:443/?token=${encodeURIComponent(moq.auth_token || "")}`,
                    name: `${deviceId}.msf`,
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
            if (session !== liveSession) return;
            ui.livePlayer.hidden = false;
            ui.moqPlayer.hidden = true;
            liveMode = "";
            setLiveButtons();
            setLiveStatus(`MoQ 启动失败：${error.message || error}`);
        }
    }

    let liveWebRtcSession = null;

    function newWebRtcSessionId() {
        const bytes = new Uint8Array(16);
        crypto.getRandomValues(bytes);
        return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
    }

    function closeWebRtcSession(deviceId, sessionId) {
        return fetch(
            `/api/v1/devices/${encodeURIComponent(deviceId)}/webrtc/offer?session_id=${encodeURIComponent(sessionId)}`,
            { method: "DELETE", cache: "no-store", keepalive: true },
        ).catch(() => {});
    }

    function stopHubLive() {
        const webRtcSession = liveWebRtcSession;
        liveWebRtcSession = null;
        liveSession += 1;
        const socket = liveSocket;
        liveSocket = null;
        try { window.CameraHubFlv && window.CameraHubFlv.stop(); } catch (_) {}
        try { window.CameraHubMoq && window.CameraHubMoq.stop(); } catch (_) {}
        clearChildren(ui.moqPlayer);
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
        if (webRtcSession) {
            void closeWebRtcSession(webRtcSession.deviceId, webRtcSession.sessionId);
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
            .replace(/&/g, "&amp;")
            .replace(/</g, "&lt;")
            .replace(/>/g, "&gt;")
            .replace(/"/g, "&quot;")
            .replace(/'/g, "&#039;");
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
    ui.voiceStudioForm.addEventListener("input", () => {
        state.voiceStudioDirty = true;
    });
    ui.voiceStudioForm.addEventListener("submit", saveVoiceStudioSettings);
    ui.copyVoiceStudioUrl.addEventListener("click", copyVoiceStudioUrl);
    if (ui.voiceTranscribeButton) {
        ui.voiceTranscribeButton.addEventListener("click", requestVoiceTranscription);
    }
    ui.openVoiceStudio.addEventListener("click", (event) => {
        if (ui.openVoiceStudio.getAttribute("aria-disabled") === "true") {
            event.preventDefault();
        }
    });
    ui.voiceServiceControls.addEventListener("click", (event) => {
        const assetButton = event.target.closest("[data-asset-install]");
        if (assetButton) {
            installVoiceAsset(assetButton.dataset.assetInstall);
            return;
        }
        const button = event.target.closest("[data-service-control]");
        if (!button) return;
        controlVoiceService(button.dataset.serviceControl, button.dataset.serviceAction);
    });
    ui.voiceServiceControls.addEventListener("change", (event) => {
        const checkbox = event.target.closest("[data-service-autostart]");
        if (!checkbox) return;
        setVoiceServiceAutostart(checkbox.dataset.serviceAutostart, checkbox.checked);
    });
    ui.irServiceControls.addEventListener("click", (event) => {
        const button = event.target.closest("[data-service-control]");
        if (!button) return;
        controlIrService(button.dataset.serviceAction);
    });
    ui.irServiceControls.addEventListener("change", (event) => {
        const checkbox = event.target.closest('[data-service-autostart="ir"]');
        if (!checkbox) return;
        setIrAutostart(checkbox.checked);
    });
    ui.airconPowerButton.addEventListener("click", () => {
        const power = !!state.aircon?.state?.power;
        sendAirconRemoteCommand(
            power ? { operation: "off" } : { operation: "power", enabled: true },
            power ? "power_off" : "power_on",
            power ? "关机" : "开机",
        );
    });
    ui.airconTempDown.addEventListener("click", () => {
        sendAirconRemoteCommand(
            { operation: "adjust_temperature", delta_c: -1 },
            "temperature_down",
            "降低温度",
        );
    });
    ui.airconTempUp.addEventListener("click", () => {
        sendAirconRemoteCommand(
            { operation: "adjust_temperature", delta_c: 1 },
            "temperature_up",
            "升高温度",
        );
    });
    document.querySelectorAll("[data-aircon-mode]").forEach((button) => {
        button.addEventListener("click", () => {
            const mode = button.dataset.airconMode;
            sendAirconRemoteCommand(
                { operation: "set_mode", mode },
                mode,
                `${AIRCON_MODE_LABELS[mode] || mode}模式`,
            );
        });
    });
    ui.airconEcoButton.addEventListener("click", () => {
        const enabled = !state.aircon?.state?.eco;
        sendAirconRemoteCommand(
            { operation: "set_eco", enabled },
            enabled ? "eco_on" : "eco_off",
            enabled ? "开启 ECO" : "关闭 ECO",
        );
    });
    ui.airconTimerButton.addEventListener("click", () => {
        const opening = ui.airconTimerPanel.hidden;
        ui.airconTimerPanel.hidden = !opening;
        ui.airconTimerButton.setAttribute("aria-expanded", String(opening));
        if (opening) ui.airconTimerOperation.focus();
    });
    ui.airconTimerPanel.addEventListener("submit", submitAirconTimer);
    ui.airconVerificationPass.addEventListener("click", () => {
        updateAirconVerification(true);
    });
    ui.airconVerificationFail.addEventListener("click", () => {
        updateAirconVerification(false);
    });
    ui.airconVerificationList.addEventListener("click", (event) => {
        const button = event.target.closest("[data-aircon-verification-reset]");
        if (!button) return;
        resetAirconVerification(button.dataset.airconVerificationReset);
    });
    ui.airconCommandForm.addEventListener("input", () => {
        state.airconDirty = true;
        updateAirconForm();
    });
    ui.airconCommandForm.addEventListener("submit", submitAirconCommand);
    ui.airconScheduleList.addEventListener("click", (event) => {
        const button = event.target.closest("[data-aircon-schedule-cancel]");
        if (!button) return;
        cancelAirconSchedule(button.dataset.airconScheduleCancel);
    });
    ui.cancelAllAirconSchedules.addEventListener("click", () => cancelAirconSchedule(""));
    ui.enableAirconIntegration.addEventListener("click", () => {
        setAirconIntegration(true);
    });
    ui.disableAirconIntegration.addEventListener("click", () => {
        setAirconIntegration(false);
    });
    ui.rotateAirconToken.addEventListener("click", rotateAirconToken);
    ui.copyAirconToken.addEventListener("click", () => {
        copyAirconValue(ui.airconIntegrationToken, "空调集成 Token 已复制");
    });
    ui.copyAirconTrigger.addEventListener("click", () => {
        copyAirconValue(ui.airconTriggerUrl, "空调触发 URL 已复制");
    });
    ui.copyAirconGuide.addEventListener("click", () => {
        copyAirconValue(ui.airconUrlGuide, "空调 URL 调用说明已复制");
    });
    ui.reloadVoice.addEventListener("click", async () => {
        state.voiceDirty = false;
        await loadVoice();
    });
    ui.addVoiceCommand.addEventListener("click", addVoiceCommand);
    ui.recordVoiceReference.addEventListener("click", startVoiceReferenceRecording);
    ui.stopVoiceReference.addEventListener("click", stopVoiceReferenceRecording);
    ui.deleteVoiceReference.addEventListener("click", deleteVoiceReference);
    ui.voiceCommandList.addEventListener("click", (event) => {
        const button = event.target.closest("[data-voice-action]");
        if (!button) return;
        const row = button.closest(".voice-command");
        if (!row) return;
        const id = row.querySelector('[data-voice-field="id"]').value;
        if (button.dataset.voiceAction === "delete") {
            row.remove();
            state.voiceDirty = true;
        } else if (button.dataset.voiceAction === "add-phrase") {
            const editor = button.closest(".voice-phrase-editor");
            const list = editor.querySelector(".voice-phrase-list");
            if (list.children.length >= 8) return;
            list.insertAdjacentHTML("beforeend", voicePhraseRow("", true));
            syncVoicePhraseButtons(editor);
            list.lastElementChild?.querySelector("[data-voice-phrase]")?.focus();
            state.voiceDirty = true;
        } else if (button.dataset.voiceAction === "remove-phrase") {
            const editor = button.closest(".voice-phrase-editor");
            if (editor.querySelectorAll(".voice-phrase-row").length <= 1) return;
            button.closest(".voice-phrase-row").remove();
            syncVoicePhraseButtons(editor);
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
    ui.copyQqCurlExample.addEventListener("click", copyQqCurlExample);
    ui.qqTestForm.addEventListener("submit", sendQqTest);
    ui.ddnsForm.addEventListener("input", () => {
        state.ddnsDirty = true;
        ui.reconcileDdns.disabled = true;
        updateDdnsCredentialRequirements();
    });
    ui.ddnsForm.addEventListener("submit", (event) => saveDdnsConfig(event));
    ui.reloadDdns.addEventListener("click", async () => {
        state.ddnsDirty = false;
        await loadDdns();
    });
    ui.addDdnsRecord.addEventListener("click", () => addDdnsRecord());
    ui.ddnsRecordList.addEventListener("click", (event) => {
        const button = event.target.closest('[data-ddns-action="delete"]');
        if (!button) return;
        button.closest(".ddns-record-row")?.remove();
        state.ddnsDirty = true;
        ui.reconcileDdns.disabled = true;
        if (!ui.ddnsRecordList.children.length) renderDdnsRecords([]);
    });
    ui.clearDdnsSecret.addEventListener("click", async () => {
        if (!state.ddnsConfig?.secret_key_configured) return;
        if (!window.confirm("清除 SecretKey 后 DDNS 会立即停用。继续？")) return;
        await saveDdnsConfig(null, true);
    });
    ui.previewDdns.addEventListener("click", previewDdns);
    ui.reconcileDdns.addEventListener("click", reconcileDdns);
    ui.ddnsServiceControls.addEventListener("click", (event) => {
        const button = event.target.closest("[data-service-control]");
        if (!button) return;
        controlDdnsComponent(button.dataset.serviceAction);
    });
    ui.ddnsServiceControls.addEventListener("change", (event) => {
        const checkbox = event.target.closest('[data-service-autostart="ddns"]');
        if (!checkbox) return;
        setDdnsAutostart(checkbox.checked);
    });
    ui.httpProxyForm.addEventListener("input", () => {
        state.httpProxyDirty = true;
    });
    ui.httpProxyForm.addEventListener("submit", saveHttpProxy);
    ui.reloadHttpProxy.addEventListener("click", async () => {
        state.httpProxyDirty = false;
        await loadHttpProxy();
    });
    ui.rotateHttpProxyToken.addEventListener("click", rotateHttpProxyToken);
    ui.copyHttpProxyToken.addEventListener("click", () => {
        copyAirconValue(ui.httpProxyToken, "HTTP 代理 Token 已复制");
    });
    ui.copyHttpProxyTemplate.addEventListener("click", () => {
        copyAirconValue(ui.httpProxyTemplate, "HTTP 代理调用示例已复制");
    });
    ui.httpProxyTestMethod.addEventListener("change", updateHttpProxyTestForm);
    ui.httpProxyTestForm.addEventListener("submit", testHttpProxy);
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

    function resolveView(view) {
        const requested = String(view || "").replace(/^#/, "");
        if (requested === "camera") return state.cameraView;
        if (requested.startsWith("camera/")) {
            const cameraView = requested.slice("camera/".length);
            return CAMERA_VIEWS.has(cameraView) ? cameraView : state.cameraView;
        }
        if (requested === "voice") return "voice";
        if (requested.startsWith("voice/")) {
            const voiceView = requested.slice("voice/".length);
            state.voiceView = VOICE_VIEWS.has(voiceView) ? voiceView : state.voiceView;
            return "voice";
        }
        if (CAMERA_VIEWS.has(requested) || ROOT_VIEWS.has(requested)) return requested;
        return "overview";
    }

    function canonicalViewHash(view) {
        if (CAMERA_VIEWS.has(view)) return `camera/${view}`;
        if (view === "voice") return `voice/${state.voiceView}`;
        return view;
    }

    function centerNavigationButton(button) {
        if (!button?.parentElement) return;
        const navigation = button.parentElement;
        navigation.scrollLeft = Math.max(
            0,
            button.offsetLeft - (navigation.clientWidth - button.offsetWidth) / 2,
        );
    }

    function activateView(view, updateHash = true) {
        const target = resolveView(view);
        const cameraActive = CAMERA_VIEWS.has(target);
        const voiceActive = target === "voice";
        const topLevelTarget = cameraActive ? "camera" : target;
        if (state.view === "live" && target !== "live") stopHubLive();
        if (state.view === "evaluation" && target !== "evaluation") {
            window.CameraHubEvaluation?.stop();
        }
        if (target !== "photos") closePhoto();
        state.view = target;
        if (cameraActive) state.cameraView = target;
        document.querySelectorAll("[data-view]").forEach((section) => {
            section.hidden = section.dataset.view !== target;
        });
        document.querySelectorAll("[data-view-target]").forEach((button) => {
            const active = button.dataset.viewTarget === topLevelTarget;
            button.classList.toggle("active", active);
            button.setAttribute("aria-current", active ? "page" : "false");
        });
        ui.cameraSubnav.hidden = !cameraActive;
        ui.voiceSubnav.hidden = !voiceActive;
        document.querySelectorAll("[data-camera-view-target]").forEach((button) => {
            const active = button.dataset.cameraViewTarget === target;
            button.classList.toggle("active", active);
            button.setAttribute("aria-selected", active ? "true" : "false");
        });
        document.querySelectorAll("[data-voice-view]").forEach((section) => {
            section.hidden = !voiceActive || section.dataset.voiceView !== state.voiceView;
        });
        document.querySelectorAll("[data-voice-view-target]").forEach((button) => {
            const active = button.dataset.voiceViewTarget === state.voiceView;
            button.classList.toggle("active", active);
            button.setAttribute("aria-selected", active ? "true" : "false");
        });
        centerNavigationButton(
            document.querySelector(`[data-view-target="${topLevelTarget}"]`),
        );
        if (cameraActive) {
            centerNavigationButton(
                document.querySelector(`[data-camera-view-target="${target}"]`),
            );
        }
        if (voiceActive) {
            centerNavigationButton(
                document.querySelector(`[data-voice-view-target="${state.voiceView}"]`),
            );
        }
        const canonicalHash = canonicalViewHash(target);
        if (updateHash || location.hash.slice(1) !== canonicalHash) {
            history.replaceState(null, "", `#${canonicalHash}`);
        }
        if (target === "playback") requestAnimationFrame(resizeTimeline);
        if (target === "photos") loadPhotos();
        if (target === "voice") loadVoice(true);
        if (target === "qq") loadQq(true);
        if (target === "ddns") loadDdns(true);
        if (target === "proxy") loadHttpProxy(true);
        if (target === "tv") loadTv(true);
        if (target === "evaluation") window.CameraHubEvaluation?.refreshDevices();
        window.scrollTo({ top: 0, behavior: "smooth" });
    }

    document.querySelectorAll("[data-view-target]").forEach((button) => {
        button.addEventListener("click", () => activateView(button.dataset.viewTarget));
    });
    document.querySelectorAll("[data-camera-view-target]").forEach((button) => {
        button.addEventListener("click", () => activateView(button.dataset.cameraViewTarget));
    });
    document.querySelectorAll("[data-voice-view-target]").forEach((button) => {
        button.addEventListener("click", () => {
            activateView(`voice/${button.dataset.voiceViewTarget}`);
        });
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
        if (state.view === "voice") loadVoice(true);
        if (state.view === "qq" && !state.qqDirty) loadQq(true);
        if (state.view === "ddns" && !state.ddnsDirty) loadDdns(true);
        if (state.view === "proxy" && !state.httpProxyDirty) loadHttpProxy(true);
        if (state.view === "tv") loadTv(true);
    }, 5_000);
    setInterval(() => {
        if (state.view === "voice" && state.voiceAssetsInstalling) {
            loadVoiceAssets(true);
        }
    }, 1_000);
    refreshAll(true);
})();
