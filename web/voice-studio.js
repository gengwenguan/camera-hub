(() => {
    "use strict";

    const $ = (id) => document.getElementById(id);
    const ui = {
        serviceStatus: $("serviceStatus"),
        recordSection: $("recordSection"),
        referencePrompt: $("referencePrompt"),
        voiceConsent: $("voiceConsent"),
        startRecording: $("startRecording"),
        stopRecording: $("stopRecording"),
        referencePreview: $("referencePreview"),
        recordStatus: $("recordStatus"),
        synthesisSection: $("synthesisSection"),
        synthesisForm: $("synthesisForm"),
        synthesisText: $("synthesisText"),
        textCount: $("textCount"),
        generateVoice: $("generateVoice"),
        synthesisStatus: $("synthesisStatus"),
        result: $("result"),
        resultAudio: $("resultAudio"),
        downloadWav: $("downloadWav"),
    };
    const state = {
        sessionToken: "",
        expiresEpoch: 0,
        enrolled: false,
        recorder: null,
        stream: null,
        chunks: [],
        recordTimer: 0,
        referenceUrl: "",
        resultUrl: "",
        sessionPending: false,
        recordingPending: false,
        enrolling: false,
    };

    function setStatus(element, message, error = false) {
        element.textContent = message;
        element.classList.toggle("error", error);
    }

    async function jsonApi(path, options) {
        const response = await fetch(path, {
            cache: "no-store",
            ...options,
            headers: {
                "Content-Type": "application/json",
                ...(options?.headers || {}),
            },
        });
        const text = await response.text();
        let body = null;
        try {
            body = text ? JSON.parse(text) : null;
        } catch (_) {
            body = null;
        }
        if (!response.ok) {
            const error = new Error(body?.error || `HTTP ${response.status}`);
            error.status = response.status;
            throw error;
        }
        return body;
    }

    function activateSession(token, prompt, expiresEpoch, enrolled = false) {
        state.sessionToken = token;
        state.expiresEpoch = expiresEpoch;
        state.enrolled = enrolled;
        sessionStorage.setItem("voiceStudioSession", JSON.stringify({
            token,
            prompt,
            expiresEpoch,
            enrolled,
        }));
        ui.referencePrompt.textContent = prompt;
        ui.recordSection.hidden = false;
        ui.synthesisSection.hidden = !enrolled;
        ui.serviceStatus.textContent = "会话已建立";
        ui.serviceStatus.className = "status active";
    }

    async function createAnonymousSession() {
        if (state.sessionPending) return;
        state.sessionPending = true;
        try {
            const response = await jsonApi("/api/v1/public/voice-studio/session", {
                method: "POST",
            });
            activateSession(
                response.session_token,
                response.reference_prompt,
                Number(response.expires_epoch),
            );
        } catch (error) {
            ui.serviceStatus.textContent = "服务不可用";
            ui.serviceStatus.className = "status error";
            setStatus(ui.recordStatus, error.message, true);
        } finally {
            state.sessionPending = false;
        }
    }

    async function recoverSession() {
        sessionStorage.removeItem("voiceStudioSession");
        state.sessionToken = "";
        state.expiresEpoch = 0;
        state.enrolled = false;
        ui.recordSection.hidden = true;
        ui.synthesisSection.hidden = true;
        ui.result.hidden = true;
        setStatus(ui.recordStatus, "会话已更新，请重新录制参考声音");
        await createAnonymousSession();
    }

    function setRecordingControls(busy, canStop = false) {
        ui.startRecording.disabled = busy;
        ui.stopRecording.disabled = !canStop;
        ui.voiceConsent.disabled = busy;
    }

    async function startRecording() {
        if (state.recordingPending || state.recorder || state.enrolling) return;
        if (!state.sessionToken) {
            await createAnonymousSession();
            if (!state.sessionToken) return;
        }
        if (!ui.voiceConsent.checked) {
            setStatus(ui.recordStatus, "请先确认声音授权", true);
            return;
        }
        if (!navigator.mediaDevices?.getUserMedia || typeof MediaRecorder !== "function") {
            setStatus(ui.recordStatus, "当前浏览器不支持录音", true);
            return;
        }
        state.recordingPending = true;
        setRecordingControls(true);
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
            state.stream = stream;
            state.recorder = recorder;
            state.chunks = chunks;
            recorder.addEventListener("dataavailable", (event) => {
                if (event.data.size) chunks.push(event.data);
            });
            recorder.addEventListener(
                "stop",
                () => enrollReference(recorder, stream, chunks),
                { once: true },
            );
            recorder.start(250);
            state.recordingPending = false;
            setRecordingControls(true, true);
            setStatus(ui.recordStatus, "正在录音，请完整朗读文稿");
            state.recordTimer = window.setTimeout(stopRecording, 15_000);
        } catch (error) {
            stopTracks(stream);
            state.recordingPending = false;
            setRecordingControls(false);
            setStatus(ui.recordStatus, error.message, true);
        }
    }

    function stopRecording() {
        clearTimeout(state.recordTimer);
        state.recordTimer = 0;
        if (state.recorder?.state === "recording") state.recorder.stop();
        setRecordingControls(true);
    }

    function stopTracks(stream = state.stream) {
        stream?.getTracks().forEach((track) => track.stop());
        if (state.stream === stream) state.stream = null;
    }

    async function enrollReference(recorder, stream, chunks) {
        state.enrolling = true;
        stopTracks(stream);
        setStatus(ui.recordStatus, "正在处理参考声音");
        try {
            const blob = new Blob(chunks, {
                type: recorder.mimeType || "audio/webm",
            });
            const wav = await audioBlobToWav(blob, 24_000);
            replaceObjectUrl("referenceUrl", wav, ui.referencePreview);
            await jsonApi("/api/v1/public/voice-studio/reference", {
                method: "PUT",
                body: JSON.stringify({
                    session_token: state.sessionToken,
                    audio_base64: await blobToBase64(wav),
                }),
            });
            state.enrolled = true;
            sessionStorage.setItem("voiceStudioSession", JSON.stringify({
                token: state.sessionToken,
                prompt: ui.referencePrompt.textContent,
                expiresEpoch: state.expiresEpoch,
                enrolled: true,
            }));
            ui.synthesisSection.hidden = false;
            setStatus(ui.recordStatus, "参考声音已录入，可以生成语音");
        } catch (error) {
            if (error.status === 401) {
                await recoverSession();
            } else {
                setStatus(ui.recordStatus, error.message, true);
            }
        } finally {
            if (state.recorder === recorder) {
                state.recorder = null;
                state.chunks = [];
            }
            state.enrolling = false;
            setRecordingControls(false);
        }
    }

    ui.synthesisForm.addEventListener("submit", async (event) => {
        event.preventDefault();
        ui.generateVoice.disabled = true;
        setStatus(ui.synthesisStatus, "正在生成，复杂设备可能需要较长时间");
        try {
            const response = await fetch("/api/v1/public/voice-studio/synthesize", {
                method: "POST",
                cache: "no-store",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({
                    session_token: state.sessionToken,
                    text: ui.synthesisText.value.trim(),
                }),
            });
            if (!response.ok) {
                const body = await response.json().catch(() => null);
                const error = new Error(body?.error || `HTTP ${response.status}`);
                error.status = response.status;
                throw error;
            }
            const wav = await response.blob();
            replaceObjectUrl("resultUrl", wav, ui.resultAudio);
            ui.downloadWav.href = state.resultUrl;
            ui.result.hidden = false;
            setStatus(ui.synthesisStatus, "生成完成");
        } catch (error) {
            if (error.status === 401) {
                await recoverSession();
                setStatus(ui.synthesisStatus, "会话已更新，请重新录入声音", true);
            } else {
                setStatus(ui.synthesisStatus, error.message, true);
            }
        } finally {
            ui.generateVoice.disabled = false;
        }
    });

    function replaceObjectUrl(key, blob, audio) {
        if (state[key]) URL.revokeObjectURL(state[key]);
        state[key] = URL.createObjectURL(blob);
        audio.src = state[key];
        audio.hidden = false;
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

    ui.startRecording.addEventListener("click", startRecording);
    ui.stopRecording.addEventListener("click", stopRecording);
    ui.synthesisText.addEventListener("input", () => {
        ui.textCount.textContent = `${Array.from(ui.synthesisText.value).length} / 120`;
    });
    window.addEventListener("pagehide", () => {
        clearTimeout(state.recordTimer);
        stopTracks();
        if (state.referenceUrl) URL.revokeObjectURL(state.referenceUrl);
        if (state.resultUrl) URL.revokeObjectURL(state.resultUrl);
    });

    let restored = false;
    try {
        const saved = JSON.parse(sessionStorage.getItem("voiceStudioSession") || "null");
        if (saved?.token?.split(".").length === 3 &&
            Number(saved.expiresEpoch) > Date.now() / 1000) {
            restored = true;
            activateSession(
                saved.token,
                saved.prompt,
                Number(saved.expiresEpoch),
                !!saved.enrolled,
            );
        }
    } catch (_) {
        sessionStorage.removeItem("voiceStudioSession");
    }
    if (!restored) createAnonymousSession();
})();
