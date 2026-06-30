import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  BadgeCheck,
  Boxes,
  CheckCircle2,
  Copy,
  FileText,
  FileArchive,
  FolderOpen,
  KeyRound,
  Loader2,
  Pencil,
  Play,
  Plus,
  RefreshCw,
  RotateCcw,
  Save,
  Shield,
  Square,
  Tags,
  Trash2,
  WandSparkles,
  X,
  XCircle
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  AppPreferences,
  ArtifactInfo,
  ChannelOptions,
  ChannelPackageResult,
  JobStatus,
  ProtectRequest,
  ProtectionOptions,
  SigningAliasInspection,
  SigningConfig,
  SigningProfile,
  SigningProfileInput,
  ToolchainPaths,
  ToolchainStatus,
  VmpOptions,
  VmpPlan
} from "./types";

const defaultVmpOptions: VmpOptions = {
  enabled: false,
  includeRules: [],
  excludeRules: [],
  maxMethodSize: 800,
  abiSelection: ["arm64-v8a", "armeabi-v7a", "x86_64"],
  unsupportedMethodPolicy: "report"
};

const defaultProtectionOptions: ProtectionOptions = {
  dexEncryption: true,
  antiDebug: true,
  signatureTamperCheck: true,
  legacyApiFallback: true
};

const availableChannels = ["huawei", "xiaomi", "oppo", "vivo", "honor", "yyb"];

const defaultChannelOptions: ChannelOptions = {
  enabled: false,
  channels: []
};

const emptyProfileDraft: SigningProfileInput = {
  id: null,
  name: "",
  keystorePath: "",
  storePassword: "",
  keyPassword: "",
  alias: "",
  storeType: "",
  signingScheme: "v1v2"
};

export default function App() {
  const [activeTab, setActiveTab] = useState<"protect" | "channels">("protect");
  const [inputPath, setInputPath] = useState("");
  const [outputDir, setOutputDir] = useState("");
  const [channelTabInputPath, setChannelTabInputPath] = useState("");
  const [channelTabOutputDir, setChannelTabOutputDir] = useState("");
  const [channelTabChannels, setChannelTabChannels] = useState<string[]>([]);
  const [channelTabResult, setChannelTabResult] = useState<ChannelPackageResult | null>(null);
  const [channelTabBusy, setChannelTabBusy] = useState(false);
  const [includeRulesText, setIncludeRulesText] = useState("");
  const [excludeRulesText, setExcludeRulesText] = useState("");
  const [vmpOptions, setVmpOptions] = useState<VmpOptions>(defaultVmpOptions);
  const [protectionOptions, setProtectionOptions] = useState<ProtectionOptions>(defaultProtectionOptions);
  const [channelOptions, setChannelOptions] = useState<ChannelOptions>(defaultChannelOptions);
  const [toolchainPaths, setToolchainPaths] = useState<ToolchainPaths>({});
  const [toolchain, setToolchain] = useState<ToolchainStatus | null>(null);
  const [artifact, setArtifact] = useState<ArtifactInfo | null>(null);
  const [vmpPlan, setVmpPlan] = useState<VmpPlan | null>(null);
  const [preferences, setPreferences] = useState<AppPreferences>({ signingProfiles: [] });
  const [selectedSigningProfileId, setSelectedSigningProfileId] = useState<string | null>(null);
  const [profileModalOpen, setProfileModalOpen] = useState(false);
  const [profileDraft, setProfileDraft] = useState<SigningProfileInput>(emptyProfileDraft);
  const [aliasInspection, setAliasInspection] = useState<SigningAliasInspection | null>(null);
  const [modalError, setModalError] = useState<string | null>(null);
  const [modalBusy, setModalBusy] = useState(false);
  const [jobId, setJobId] = useState<string | null>(null);
  const [job, setJob] = useState<JobStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const openedOutputJobs = useRef<Set<string>>(new Set());

  const selectedSigningProfile = useMemo(
    () => preferences.signingProfiles.find((profile) => profile.id === selectedSigningProfileId) ?? null,
    [preferences.signingProfiles, selectedSigningProfileId]
  );

  const request = useMemo<ProtectRequest>(() => {
    const resolvedVmp: VmpOptions = {
      ...vmpOptions,
      includeRules: splitRules(includeRulesText),
      excludeRules: splitRules(excludeRulesText)
    };
    return {
      inputPath,
      outputDir,
      artifactKind: artifact?.kind ?? null,
      vmpOptions: resolvedVmp,
      protectionOptions,
      channelOptions,
      signingConfig: null,
      signingProfileId: selectedSigningProfileId,
      toolchainPaths
    };
  }, [artifact?.kind, channelOptions, excludeRulesText, includeRulesText, inputPath, outputDir, protectionOptions, selectedSigningProfileId, toolchainPaths, vmpOptions]);

  const applyPreferences = useCallback((next: AppPreferences) => {
    setPreferences(next);
    setSelectedSigningProfileId(next.selectedSigningProfileId ?? next.signingProfiles[0]?.id ?? null);
    if (next.lastOutputDir) {
      setOutputDir((current) => current || next.lastOutputDir || "");
      setChannelTabOutputDir((current) => current || next.lastOutputDir || "");
    }
  }, []);

  const loadPreferences = useCallback(async () => {
    try {
      applyPreferences(await invoke<AppPreferences>("load_app_preferences"));
    } catch (err) {
      setError(String(err));
    }
  }, [applyPreferences]);

  const detectToolchain = useCallback(async () => {
    try {
      setToolchain(await invoke<ToolchainStatus>("detect_toolchain", { paths: toolchainPaths }));
    } catch (err) {
      setError(String(err));
    }
  }, [toolchainPaths]);

  useEffect(() => {
    detectToolchain();
    loadPreferences();
  }, [detectToolchain, loadPreferences]);

  useEffect(() => {
    if (!jobId) return;
    const timer = window.setInterval(async () => {
      try {
        const status = await invoke<JobStatus>("get_job_status", { jobId });
        setJob(status);
        if (status.lifecycle === "succeeded" && !openedOutputJobs.current.has(status.id)) {
          openedOutputJobs.current.add(status.id);
          const pathToOpen = status.outputPath || outputDir;
          if (pathToOpen) {
            void invoke("open_output_dir", { path: pathToOpen }).catch((err) => setError(String(err)));
          }
        }
        if (["succeeded", "failed", "canceled"].includes(status.lifecycle)) {
          window.clearInterval(timer);
        }
      } catch (err) {
        setError(String(err));
        window.clearInterval(timer);
      }
    }, 750);
    return () => window.clearInterval(timer);
  }, [jobId, outputDir]);

  const scan = useCallback(async () => {
    if (!inputPath) return;
    setBusy(true);
    setError(null);
    try {
      const info = await invoke<ArtifactInfo>("scan_artifact", { path: inputPath });
      setArtifact(info);
      setVmpPlan(null);
    } catch (err) {
      setArtifact(null);
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }, [inputPath]);

  const estimateVmp = useCallback(async () => {
    if (!inputPath) return;
    setBusy(true);
    setError(null);
    try {
      setVmpPlan(await invoke<VmpPlan>("estimate_vmp", { request }));
    } catch (err) {
      setVmpPlan(null);
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }, [inputPath, request]);

  const startProtection = useCallback(async () => {
    if (!inputPath || !outputDir || !selectedSigningProfileId) return;
    setBusy(true);
    setError(null);
    setJob(null);
    try {
      const id = await invoke<string>("protect_artifact", { request });
      setJobId(id);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }, [inputPath, outputDir, request, selectedSigningProfileId]);

  const cancelJob = useCallback(async () => {
    if (!jobId) return;
    await invoke<boolean>("cancel_job", { jobId });
  }, [jobId]);

  const openOutputPath = useCallback(
    async (path?: string | null) => {
      const pathToOpen = path || outputDir;
      if (!pathToOpen) return;
      try {
        await invoke("open_output_dir", { path: pathToOpen });
      } catch (err) {
        setError(String(err));
      }
    },
    [outputDir]
  );

  const copyOutputPath = useCallback(async (path?: string | null) => {
    if (!path) return;
    try {
      await navigator.clipboard.writeText(path);
    } catch (err) {
      setError(`复制路径失败：${String(err)}`);
    }
  }, []);

  const saveJobLog = useCallback(
    async (targetJob: JobStatus) => {
      const fileName = `android-protector-${targetJob.id.slice(0, 8)}.log`;
      const defaultPath = outputDir ? `${outputDir.replace(/[\\/]+$/, "")}/${fileName}` : fileName;
      try {
        const selected = await save({
          defaultPath,
          filters: [{ name: "Log", extensions: ["log", "txt"] }]
        });
        if (typeof selected === "string") {
          await invoke("save_job_log", { jobId: targetJob.id, path: selected });
        }
      } catch (err) {
        setError(String(err));
      }
    },
    [outputDir]
  );

  const chooseSigningProfile = async (id: string) => {
    setSelectedSigningProfileId(id);
    try {
      applyPreferences(await invoke<AppPreferences>("set_selected_signing_profile", { id }));
    } catch (err) {
      setError(String(err));
    }
  };

  const openCreateProfile = () => {
    setProfileDraft({ ...emptyProfileDraft });
    setAliasInspection(null);
    setModalError(null);
    setProfileModalOpen(true);
  };

  const openEditProfile = async (profile: SigningProfile) => {
    setModalBusy(true);
    setModalError(null);
    try {
      const draft = await invoke<SigningProfileInput>("get_signing_profile_input", { id: profile.id });
      setProfileDraft({
        ...draft,
        storeType: draft.storeType ?? "",
        keyPassword: draft.keyPassword ?? "",
        signingScheme: draft.signingScheme ?? "v1v2"
      });
      setAliasInspection({
        valid: true,
        storeType: draft.storeType ?? profile.storeType ?? null,
        aliases: [
          {
            alias: draft.alias,
            entryType: null,
            certificateSummary: profile.certificateSummary ?? null
          }
        ],
        issues: []
      });
      setProfileModalOpen(true);
    } catch (err) {
      setError(String(err));
    } finally {
      setModalBusy(false);
    }
  };

  const deleteProfile = async (profile: SigningProfile) => {
    if (!window.confirm(`删除签名信息「${profile.name}」？`)) return;
    try {
      applyPreferences(await invoke<AppPreferences>("delete_signing_profile", { id: profile.id }));
    } catch (err) {
      setError(String(err));
    }
  };

  const inspectAliases = async () => {
    setModalBusy(true);
    setModalError(null);
    try {
      const config = profileDraftToSigningConfig({ ...profileDraft, alias: "" });
      const inspection = await invoke<SigningAliasInspection>("inspect_signing_aliases", { config });
      setAliasInspection(inspection);
      if (!inspection.valid) {
        setModalError(inspection.issues.join("\n") || "未读取到别名");
      }
      if (inspection.valid && inspection.aliases.length > 0) {
        setProfileDraft((current) => ({
          ...current,
          alias: inspection.aliases.some((item) => item.alias === current.alias)
            ? current.alias
            : inspection.aliases[0].alias,
          storeType: inspection.storeType ?? current.storeType ?? ""
        }));
      } else {
        setProfileDraft((current) => ({ ...current, alias: "" }));
      }
    } catch (err) {
      setModalError(String(err));
    } finally {
      setModalBusy(false);
    }
  };

  useEffect(() => {
    if (!profileModalOpen || !profileDraft.keystorePath || !profileDraft.storePassword) {
      return;
    }

    const timer = window.setTimeout(async () => {
      setModalBusy(true);
      setModalError(null);
      try {
        const config = profileDraftToSigningConfig({ ...profileDraft, alias: "" });
        const inspection = await invoke<SigningAliasInspection>("inspect_signing_aliases", { config });
        setAliasInspection(inspection);
        if (inspection.valid && inspection.aliases.length > 0) {
          setProfileDraft((current) => ({
            ...current,
            alias: inspection.aliases.some((item) => item.alias === current.alias)
              ? current.alias
              : inspection.aliases[0].alias,
            storeType: inspection.storeType ?? current.storeType ?? ""
          }));
        } else {
          setProfileDraft((current) => ({ ...current, alias: "" }));
          setModalError(inspection.issues.join("\n") || "未读取到别名");
        }
      } catch (err) {
        setModalError(String(err));
      } finally {
        setModalBusy(false);
      }
    }, 500);

    return () => window.clearTimeout(timer);
  }, [profileModalOpen, profileDraft.keystorePath, profileDraft.storePassword]);

  const saveProfile = async () => {
    if (!profileDraft.alias) {
      setModalError("请先读取并选择别名");
      return;
    }
    if (!profileDraft.keyPassword) {
      setModalError("请输入别名密钥");
      return;
    }
    if (!aliasInspection?.aliases.some((item) => item.alias === profileDraft.alias)) {
      setModalError("别名必须来自当前签名文件的读取结果");
      return;
    }
    setModalBusy(true);
    setModalError(null);
    try {
      const next = await invoke<AppPreferences>("save_signing_profile", { input: profileDraft });
      applyPreferences(next);
      setProfileModalOpen(false);
    } catch (err) {
      setModalError(String(err));
    } finally {
      setModalBusy(false);
    }
  };

  const selectInput = async () => {
    const selected = await open({
      multiple: false,
      filters: [{ name: "Android", extensions: ["apk", "aab"] }]
    });
    if (typeof selected === "string") setInputPath(selected);
  };

  const selectOutputDir = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected !== "string") return;
    setOutputDir(selected);
    try {
      applyPreferences(await invoke<AppPreferences>("save_last_output_dir", { path: selected }));
    } catch (err) {
      setError(String(err));
    }
  };

  const selectChannelTabInput = async () => {
    const selected = await open({
      multiple: false,
      filters: [{ name: "APK", extensions: ["apk"] }]
    });
    if (typeof selected === "string") {
      setChannelTabInputPath(selected);
      setChannelTabResult(null);
    }
  };

  const selectChannelTabOutputDir = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected === "string") {
      setChannelTabOutputDir(selected);
      setChannelTabResult(null);
    }
  };

  const selectProfileKeystore = async () => {
    const selected = await open({
      multiple: false,
      filters: [{ name: "Keystore", extensions: ["jks", "keystore", "p12", "pfx"] }]
    });
    if (typeof selected === "string") {
      setProfileDraft((current) => ({ ...current, keystorePath: selected, alias: "" }));
      setAliasInspection(null);
    }
  };

  const toggleChannel = (channel: string, checked: boolean) => {
    setChannelOptions((current) => ({
      ...current,
      channels: checked
        ? [...current.channels, channel].filter((value, index, values) => values.indexOf(value) === index)
        : current.channels.filter((value) => value !== channel)
    }));
  };

  const toggleChannelTabChannel = (channel: string, checked: boolean) => {
    setChannelTabChannels((current) =>
      checked
        ? [...current, channel].filter((value, index, values) => values.indexOf(value) === index)
        : current.filter((value) => value !== channel)
    );
  };

  const packageChannelTab = useCallback(async () => {
    if (!channelTabInputPath || !channelTabOutputDir || channelTabChannels.length === 0) return;
    setChannelTabBusy(true);
    setChannelTabResult(null);
    setError(null);
    try {
      const result = await invoke<ChannelPackageResult>("package_channels", {
        inputPath: channelTabInputPath,
        outputDir: channelTabOutputDir,
        channels: channelTabChannels
      });
      setChannelTabResult(result);
      if (result.outputDir) {
        void invoke("open_output_dir", { path: result.outputDir }).catch((err) => setError(String(err)));
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setChannelTabBusy(false);
    }
  }, [channelTabChannels, channelTabInputPath, channelTabOutputDir]);

  const running = job?.lifecycle === "running" || job?.lifecycle === "queued";
  const inferredInputKind = inferArtifactKind(inputPath);
  const channelPackagingDisabled = (artifact?.kind ?? inferredInputKind) === "aab";
  const channelSelectionValid =
    !channelOptions.enabled || (channelOptions.channels.length > 0 && !channelPackagingDisabled);
  const canStart = Boolean(inputPath && outputDir && selectedSigningProfileId && channelSelectionValid) && !running;
  const canPackageChannels =
    inferArtifactKind(channelTabInputPath) === "apk" &&
    Boolean(channelTabOutputDir && channelTabChannels.length > 0) &&
    !channelTabBusy;

  useEffect(() => {
    if (!channelPackagingDisabled) return;
    setChannelOptions((current) =>
      current.enabled || current.channels.length > 0 ? defaultChannelOptions : current
    );
  }, [channelPackagingDisabled]);

  return (
    <main className="app-shell">
      <header className="topbar">
        <div>
          <div className="product-mark">
            <Shield size={24} />
            <span>Android 第三代加固工具</span>
          </div>
          <div className="subline">APK / AAB · VMP · DEX payload · 签名验证</div>
        </div>
        <div className={`status-pill ${toolchain?.ok ? "ok" : "warn"}`}>
          {toolchain?.ok ? <CheckCircle2 size={16} /> : <AlertTriangle size={16} />}
          <span>{toolchain?.ok ? "工具链就绪" : "工具链待配置"}</span>
        </div>
      </header>

      {error && (
        <section className="notice error">
          <XCircle size={18} />
          <span>{error}</span>
        </section>
      )}

      <nav className="top-tabs">
        <button className={activeTab === "protect" ? "active" : ""} onClick={() => setActiveTab("protect")}>
          <Shield size={16} />
          加固
        </button>
        <button className={activeTab === "channels" ? "active" : ""} onClick={() => setActiveTab("channels")}>
          <Tags size={16} />
          渠道包
        </button>
      </nav>

      {activeTab === "protect" ? (
      <section className="layout-grid">
        <div className="main-column">
          <Panel icon={<FileArchive size={18} />} title="输入包">
            <div className="path-row">
              <input value={inputPath} onChange={(event) => setInputPath(event.target.value)} placeholder="APK 或 AAB 路径" />
              <button className="icon-button" onClick={selectInput} title="选择输入包">
                <FolderOpen size={18} />
              </button>
              <button className="secondary" onClick={scan} disabled={!inputPath || busy}>
                <RefreshCw size={16} />
                扫描
              </button>
            </div>
            {artifact && <ArtifactSummary artifact={artifact} />}
          </Panel>

          <Panel icon={<WandSparkles size={18} />} title="VMP">
            <div className="switch-row">
              <label className="switch">
                <input
                  type="checkbox"
                  checked={vmpOptions.enabled}
                  onChange={(event) => setVmpOptions((prev) => ({ ...prev, enabled: event.target.checked }))}
                />
                <span />
              </label>
              <strong>{vmpOptions.enabled ? "启用" : "关闭"}</strong>
              <button className="secondary compact" onClick={estimateVmp} disabled={!inputPath || busy}>
                <Boxes size={16} />
                估算
              </button>
            </div>
            <div className="rule-grid">
              <label>
                Include
                <textarea value={includeRulesText} onChange={(event) => setIncludeRulesText(event.target.value)} placeholder="com.example.pay&#10;Security::checkToken" />
              </label>
              <label>
                Exclude
                <textarea value={excludeRulesText} onChange={(event) => setExcludeRulesText(event.target.value)} placeholder="com.example.debug&#10;MainActivity::onCreate" />
              </label>
              <label>
                Max method size
                <input
                  type="number"
                  min={32}
                  max={20000}
                  value={vmpOptions.maxMethodSize}
                  onChange={(event) => setVmpOptions((prev) => ({ ...prev, maxMethodSize: Number(event.target.value) }))}
                />
              </label>
              <label>
                ABI
                <input
                  value={vmpOptions.abiSelection.join(", ")}
                  onChange={(event) =>
                    setVmpOptions((prev) => ({
                      ...prev,
                      abiSelection: splitRules(event.target.value)
                    }))
                  }
                />
              </label>
            </div>
            {vmpPlan && <VmpPlanView plan={vmpPlan} />}
          </Panel>

          <Panel icon={<Shield size={18} />} title="加固选项">
            <div className="toggle-grid">
              <Toggle label="DEX 加密" checked={protectionOptions.dexEncryption} onChange={(value) => setProtectionOptions((prev) => ({ ...prev, dexEncryption: value }))} />
              <Toggle label="反调试" checked={protectionOptions.antiDebug} onChange={(value) => setProtectionOptions((prev) => ({ ...prev, antiDebug: value }))} />
              <Toggle label="签名防篡改" checked={protectionOptions.signatureTamperCheck} onChange={(value) => setProtectionOptions((prev) => ({ ...prev, signatureTamperCheck: value }))} />
              <Toggle label="Legacy fallback" checked={protectionOptions.legacyApiFallback} onChange={(value) => setProtectionOptions((prev) => ({ ...prev, legacyApiFallback: value }))} />
            </div>
          </Panel>

          <Panel icon={<KeyRound size={18} />} title="签名信息">
            <button className="secondary full" onClick={openCreateProfile}>
              <Plus size={16} />
              新增签名
            </button>
            <SigningProfileList
              profiles={preferences.signingProfiles}
              selectedId={selectedSigningProfileId}
              onSelect={chooseSigningProfile}
              onEdit={openEditProfile}
              onDelete={deleteProfile}
            />
          </Panel>

          <Panel icon={<Tags size={18} />} title="多渠道打包">
            <div className="switch-row">
              <label className={`switch ${channelPackagingDisabled ? "disabled" : ""}`}>
                <input
                  type="checkbox"
                  checked={channelOptions.enabled && !channelPackagingDisabled}
                  disabled={channelPackagingDisabled}
                  onChange={(event) =>
                    setChannelOptions((prev) => ({
                      ...prev,
                      enabled: event.target.checked,
                      channels: event.target.checked ? prev.channels : []
                    }))
                  }
                />
                <span />
              </label>
              <strong>{channelOptions.enabled && !channelPackagingDisabled ? "启用" : "关闭"}</strong>
            </div>
            <div className={`channel-grid ${!channelOptions.enabled || channelPackagingDisabled ? "disabled" : ""}`}>
              {availableChannels.map((channel) => (
                <label className="channel-item" key={channel}>
                  <input
                    type="checkbox"
                    checked={channelOptions.channels.includes(channel)}
                    disabled={!channelOptions.enabled || channelPackagingDisabled}
                    onChange={(event) => toggleChannel(channel, event.target.checked)}
                  />
                  <span>{channel}</span>
                </label>
              ))}
            </div>
            {channelPackagingDisabled && <div className="inline-hint">AAB 文件不支持多渠道打包，请使用 APK。</div>}
            {channelOptions.enabled && !channelPackagingDisabled && channelOptions.channels.length === 0 && (
              <div className="inline-hint">请至少选择一个渠道。</div>
            )}
          </Panel>

          <Panel icon={<FolderOpen size={18} />} title="输出目录">
            <div className="path-row solo">
              <input value={outputDir} onChange={(event) => setOutputDir(event.target.value)} placeholder="输出目录" />
              <button className="icon-button" onClick={selectOutputDir} title="选择输出目录">
                <FolderOpen size={18} />
              </button>
            </div>
            {preferences.lastOutputDir && <div className="inline-hint">上次使用：{preferences.lastOutputDir}</div>}
            {job?.outputPath && <div className="output-path">{job.outputPath}</div>}
          </Panel>

          <Panel icon={<Loader2 size={18} />} title="任务">
            <div className="action-row">
              <button className="primary" onClick={startProtection} disabled={!canStart || busy}>
                <Play size={17} />
                开始加固
              </button>
              <button className="secondary" onClick={cancelJob} disabled={!running}>
                <Square size={15} />
                取消
              </button>
              {job && <span className={`job-state ${job.lifecycle}`}>{job.lifecycle}</span>}
            </div>
            {!selectedSigningProfile && <div className="inline-hint">请选择一个已保存并校验通过的签名信息。</div>}
            {job && (
              <JobView
                job={job}
                canRetry={canStart}
                onOpenOutput={openOutputPath}
                onCopyOutput={copyOutputPath}
                onSaveLog={saveJobLog}
                onRetry={startProtection}
              />
            )}
          </Panel>
        </div>

        <aside className="side-column">
          <Panel icon={<RefreshCw size={18} />} title="工具链">
            <button className="secondary full" onClick={detectToolchain}>
              <RefreshCw size={16} />
              重新探测
            </button>
            <ToolchainView toolchain={toolchain} />
            <details className="advanced-toolchain">
              <summary>高级路径覆盖</summary>
              <div className="stacked-fields">
                <input value={toolchainPaths.androidSdk ?? ""} onChange={(event) => setToolchainPaths((prev) => ({ ...prev, androidSdk: event.target.value }))} placeholder="Android SDK" />
                <input value={toolchainPaths.javaHome ?? ""} onChange={(event) => setToolchainPaths((prev) => ({ ...prev, javaHome: event.target.value }))} placeholder="JAVA_HOME" />
                <input value={toolchainPaths.bundletool ?? ""} onChange={(event) => setToolchainPaths((prev) => ({ ...prev, bundletool: event.target.value }))} placeholder="bundletool.jar" />
              </div>
            </details>
          </Panel>
        </aside>
      </section>
      ) : (
        <ChannelPackageWorkspace
          inputPath={channelTabInputPath}
          outputDir={channelTabOutputDir}
          channels={channelTabChannels}
          busy={channelTabBusy}
          canStart={canPackageChannels}
          result={channelTabResult}
          onInputChange={(value) => {
            setChannelTabInputPath(value);
            setChannelTabResult(null);
          }}
          onOutputDirChange={(value) => {
            setChannelTabOutputDir(value);
            setChannelTabResult(null);
          }}
          onSelectInput={selectChannelTabInput}
          onSelectOutputDir={selectChannelTabOutputDir}
          onToggleChannel={toggleChannelTabChannel}
          onPackage={packageChannelTab}
          onOpenOutput={openOutputPath}
          onCopyPath={copyOutputPath}
        />
      )}

      {profileModalOpen && (
        <SigningProfileModal
          draft={profileDraft}
          aliasInspection={aliasInspection}
          busy={modalBusy}
          error={modalError}
          onDraftChange={setProfileDraft}
          onClose={() => setProfileModalOpen(false)}
          onSelectKeystore={selectProfileKeystore}
          onInspectAliases={inspectAliases}
          onSave={saveProfile}
        />
      )}
    </main>
  );
}

function SigningProfileList({
  profiles,
  selectedId,
  onSelect,
  onEdit,
  onDelete
}: {
  profiles: SigningProfile[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onEdit: (profile: SigningProfile) => void;
  onDelete: (profile: SigningProfile) => void;
}) {
  if (profiles.length === 0) {
    return <div className="empty-state">暂无已保存签名。</div>;
  }

  return (
    <div className="profile-list">
      {profiles.map((profile) => (
        <div className={`profile-item ${profile.id === selectedId ? "selected" : ""}`} key={profile.id}>
          <label className="profile-main">
            <input type="radio" checked={profile.id === selectedId} onChange={() => onSelect(profile.id)} />
            <span>
              <strong>{profile.name}</strong>
              <small>{profile.alias} · {formatSigningScheme(profile.signingScheme)}</small>
              <code>{profile.keystorePath}</code>
            </span>
          </label>
          <div className="profile-actions">
            <button className="icon-button small" onClick={() => onEdit(profile)} title="编辑签名">
              <Pencil size={15} />
            </button>
            <button className="icon-button small danger" onClick={() => onDelete(profile)} title="删除签名">
              <Trash2 size={15} />
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}

function SigningProfileModal({
  draft,
  aliasInspection,
  busy,
  error,
  onDraftChange,
  onClose,
  onSelectKeystore,
  onInspectAliases,
  onSave
}: {
  draft: SigningProfileInput;
  aliasInspection: SigningAliasInspection | null;
  busy: boolean;
  error: string | null;
  onDraftChange: (draft: SigningProfileInput) => void;
  onClose: () => void;
  onSelectKeystore: () => void;
  onInspectAliases: () => void;
  onSave: () => void;
}) {
  const aliases = aliasInspection?.aliases ?? [];
  const canSave = Boolean(draft.keystorePath && draft.storePassword && draft.keyPassword && draft.alias && aliasInspection?.valid) && !busy;

  return (
    <div className="modal-backdrop">
      <section className="modal">
        <header className="modal-header">
          <div>
            <h2>{draft.id ? "编辑签名信息" : "新增签名信息"}</h2>
            <p>选择签名文件，输入签名文件密钥后会自动识别类型和别名。</p>
          </div>
          <button className="icon-button small" onClick={onClose} title="关闭">
            <X size={16} />
          </button>
        </header>

        {error && (
          <div className="notice error modal-error">
            <XCircle size={16} />
            <span>{error}</span>
          </div>
        )}

        <div className="modal-fields">
          <label>
            名称
            <input value={draft.name} onChange={(event) => onDraftChange({ ...draft, name: event.target.value })} placeholder="例如：Release 上传签名" />
          </label>
          <label>
            签名文件
            <div className="path-row solo">
              <input value={draft.keystorePath} onChange={(event) => onDraftChange({ ...draft, keystorePath: event.target.value, alias: "" })} placeholder="keystore / jks / p12" />
              <button className="icon-button" onClick={onSelectKeystore} title="选择签名文件">
                <FolderOpen size={18} />
              </button>
            </div>
          </label>
          <div className="rule-grid modal-rule-grid">
            <label>
              Store password
              <input type="text" value={draft.storePassword} onChange={(event) => onDraftChange({ ...draft, storePassword: event.target.value, alias: "" })} />
            </label>
            <label>
              Key password
              <input type="text" value={draft.keyPassword ?? ""} onChange={(event) => onDraftChange({ ...draft, keyPassword: event.target.value })} />
            </label>
            <label>
              Store type
              <input value={draft.storeType || aliasInspection?.storeType || "自动识别"} readOnly />
            </label>
            <label>
              Alias
              <select value={draft.alias} onChange={(event) => onDraftChange({ ...draft, alias: event.target.value })} disabled={aliases.length === 0}>
                <option value="">请选择别名</option>
                {aliases.map((item) => (
                  <option value={item.alias} key={item.alias}>
                    {item.alias}
                  </option>
                ))}
              </select>
            </label>
            <label>
              签名方式
              <select value={draft.signingScheme ?? "v1v2"} onChange={(event) => onDraftChange({ ...draft, signingScheme: event.target.value as SigningProfileInput["signingScheme"] })}>
                <option value="v1v2">V1 + V2</option>
                <option value="v1v2v3">V1 + V2 + V3</option>
              </select>
            </label>
          </div>
          {aliases.length > 0 && (
            <div className="alias-list">
              {aliases.map((item) => (
                <div key={item.alias}>
                  <strong>{item.alias}</strong>
                  <span>{item.entryType ?? "entry"}</span>
                </div>
              ))}
            </div>
          )}
        </div>

        <footer className="modal-actions">
          <button className="secondary" onClick={onInspectAliases} disabled={busy || !draft.keystorePath || !draft.storePassword}>
            <BadgeCheck size={16} />
            重新识别别名
          </button>
          <button className="primary" onClick={onSave} disabled={!canSave}>
            <Save size={16} />
            保存
          </button>
        </footer>
      </section>
    </div>
  );
}

function Panel({ icon, title, children }: { icon: React.ReactNode; title: string; children: React.ReactNode }) {
  return (
    <section className="panel">
      <div className="panel-title">
        {icon}
        <h2>{title}</h2>
      </div>
      {children}
    </section>
  );
}

function Toggle({ label, checked, onChange }: { label: string; checked: boolean; onChange: (checked: boolean) => void }) {
  return (
    <label className="toggle-item">
      <input type="checkbox" checked={checked} onChange={(event) => onChange(event.target.checked)} />
      <span>{label}</span>
    </label>
  );
}

function ChannelPackageWorkspace({
  inputPath,
  outputDir,
  channels,
  busy,
  canStart,
  result,
  onInputChange,
  onOutputDirChange,
  onSelectInput,
  onSelectOutputDir,
  onToggleChannel,
  onPackage,
  onOpenOutput,
  onCopyPath
}: {
  inputPath: string;
  outputDir: string;
  channels: string[];
  busy: boolean;
  canStart: boolean;
  result: ChannelPackageResult | null;
  onInputChange: (value: string) => void;
  onOutputDirChange: (value: string) => void;
  onSelectInput: () => void;
  onSelectOutputDir: () => void;
  onToggleChannel: (channel: string, checked: boolean) => void;
  onPackage: () => void;
  onOpenOutput: (path?: string | null) => void;
  onCopyPath: (path?: string | null) => void;
}) {
  const inputKind = inferArtifactKind(inputPath);
  const unsupportedInput = inputPath.trim().length > 0 && inputKind !== "apk";

  return (
    <section className="single-column">
      <Panel icon={<FileArchive size={18} />} title="原始包">
        <div className="path-row solo">
          <input value={inputPath} onChange={(event) => onInputChange(event.target.value)} placeholder="已签名 APK 路径" />
          <button className="icon-button" onClick={onSelectInput} title="选择 APK">
            <FolderOpen size={18} />
          </button>
        </div>
        {unsupportedInput && <div className="inline-hint">渠道包只支持 APK，不支持 AAB 或其他文件。</div>}
      </Panel>

      <Panel icon={<FolderOpen size={18} />} title="渠道包输出目录">
        <div className="path-row solo">
          <input value={outputDir} onChange={(event) => onOutputDirChange(event.target.value)} placeholder="渠道包输出目录" />
          <button className="icon-button" onClick={onSelectOutputDir} title="选择输出目录">
            <FolderOpen size={18} />
          </button>
        </div>
      </Panel>

      <Panel icon={<Tags size={18} />} title="渠道选择">
        <div className="channel-grid">
          {availableChannels.map((channel) => (
            <label className="channel-item" key={channel}>
              <input type="checkbox" checked={channels.includes(channel)} onChange={(event) => onToggleChannel(channel, event.target.checked)} />
              <span>{channel}</span>
            </label>
          ))}
        </div>
        {channels.length === 0 && <div className="inline-hint">请至少选择一个渠道。</div>}
      </Panel>

      <Panel icon={<Loader2 size={18} />} title="任务">
        <div className="action-row">
          <button className="primary" onClick={onPackage} disabled={!canStart}>
            {busy ? <Loader2 size={17} className="spin" /> : <Play size={17} />}
            生成渠道包
          </button>
          {result && <span className="job-state succeeded">succeeded</span>}
        </div>
        {result && (
          <div className="channel-result">
            <div className="result-actions">
              <button className="secondary" onClick={() => onOpenOutput(result.outputDir)}>
                <FolderOpen size={16} />
                打开目录
              </button>
              <button className="secondary" onClick={() => onCopyPath(result.outputDir)}>
                <Copy size={16} />
                复制目录
              </button>
            </div>
            <div className="package-list">
              {result.packages.map((item) => (
                <div key={item.channel}>
                  <strong>{item.channel}</strong>
                  <code>{item.path}</code>
                </div>
              ))}
            </div>
          </div>
        )}
      </Panel>
    </section>
  );
}

function ArtifactSummary({ artifact }: { artifact: ArtifactInfo }) {
  const totalMethods = artifact.dexFiles.reduce((sum, dex) => sum + dex.methodCount, 0);
  const virtualizable = artifact.dexFiles.reduce((sum, dex) => sum + dex.virtualizableMethods, 0);
  return (
    <div className="summary-grid">
      <Metric label="类型" value={artifact.kind.toUpperCase()} />
      <Metric label="包名" value={artifact.packageName ?? "unknown"} />
      <Metric label="DEX" value={String(artifact.dexFiles.length)} />
      <Metric label="方法" value={formatNumber(totalMethods)} />
      <Metric label="VMP 候选" value={formatNumber(virtualizable)} />
      <Metric label="ABI" value={artifact.nativeAbis.length ? artifact.nativeAbis.join(", ") : "none"} />
      {artifact.warnings.length > 0 && <div className="warning-list">{artifact.warnings.map((warning) => <div key={warning}>{warning}</div>)}</div>}
    </div>
  );
}

function VmpPlanView({ plan }: { plan: VmpPlan }) {
  return (
    <div className="vmp-plan">
      <div className="metric-strip">
        <Metric label="将虚拟化" value={formatNumber(plan.virtualizedMethods)} />
        <Metric label="跳过" value={formatNumber(plan.skippedMethods)} />
        <Metric label="风险" value={plan.riskLevel} />
      </div>
      {plan.skippedReasons.length > 0 && (
        <div className="reason-list">
          {plan.skippedReasons.slice(0, 6).map((item) => (
            <div key={item.reason}>
              <strong>{item.reason}</strong>
              <span>{item.count}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function ToolchainView({ toolchain }: { toolchain: ToolchainStatus | null }) {
  if (!toolchain) return <div className="muted">未探测</div>;
  return (
    <div className="tool-list">
      <ToolRow name="Java" ok={toolchain.java.available} value={toolchain.java.version ?? toolchain.java.path ?? ""} />
      <ToolRow name="SDK" ok={Boolean(toolchain.androidSdk)} value={toolchain.androidSdk ?? ""} />
      <ToolRow name="Build Tools" ok={Boolean(toolchain.selectedBuildTools)} value={toolchain.selectedBuildTools?.version ?? ""} />
      <ToolRow name="zipalign" ok={toolchain.zipalign.available} value={toolchain.zipalign.path ?? ""} />
      <ToolRow name="apksigner" ok={toolchain.apksigner.available} value={toolchain.apksigner.path ?? ""} />
      {toolchain.issues.length > 0 && <div className="issue-list">{toolchain.issues.map((issue) => <div key={issue}>{issue}</div>)}</div>}
    </div>
  );
}

function ToolRow({ name, ok, value }: { name: string; ok: boolean; value: string }) {
  return (
    <div className="tool-row">
      {ok ? <CheckCircle2 size={15} /> : <AlertTriangle size={15} />}
      <span>{name}</span>
      <code>{value || "missing"}</code>
    </div>
  );
}

function JobView({
  job,
  canRetry,
  onOpenOutput,
  onCopyOutput,
  onSaveLog,
  onRetry
}: {
  job: JobStatus;
  canRetry: boolean;
  onOpenOutput: (path?: string | null) => void;
  onCopyOutput: (path?: string | null) => void;
  onSaveLog: (job: JobStatus) => void;
  onRetry: () => void;
}) {
  const terminal = ["succeeded", "failed", "canceled"].includes(job.lifecycle);
  const suggestions = buildFailureSuggestions(job);

  return (
    <div className="job-view">
      <div className="progress-track">
        <div style={{ width: `${Math.round(job.progress * 100)}%` }} />
      </div>
      <div className="job-meta">
        <span>{job.stage}</span>
        <span>{Math.round(job.progress * 100)}%</span>
      </div>
      <div className="log-box">
        {job.logs.slice(-12).map((entry, index) => (
          <div key={`${entry.timestamp}-${index}`}>
            <span>{entry.stage}</span>
            <p>{entry.message}</p>
          </div>
        ))}
      </div>
      {terminal && (
        <div className="result-actions">
          <button className="secondary" onClick={() => onOpenOutput(job.outputPath)} disabled={!job.outputPath}>
            <FolderOpen size={16} />
            打开目录
          </button>
          <button className="secondary" onClick={() => onCopyOutput(job.outputPath)} disabled={!job.outputPath}>
            <Copy size={16} />
            复制路径
          </button>
          <button className="secondary" onClick={() => onSaveLog(job)}>
            <FileText size={16} />
            保存日志
          </button>
          <button className="secondary" onClick={onRetry} disabled={!canRetry}>
            <RotateCcw size={16} />
            重新加固
          </button>
        </div>
      )}
      {job.outputPath && (
        <div className="result-path">
          <strong>输出文件</strong>
          <code>{job.outputPath}</code>
        </div>
      )}
      {job.error && (
        <div className="job-error">
          <strong>错误</strong>
          <p>{job.error}</p>
        </div>
      )}
      {suggestions.length > 0 && (
        <div className="repair-hints">
          <strong>修复建议</strong>
          {suggestions.map((suggestion) => (
            <p key={suggestion}>{suggestion}</p>
          ))}
        </div>
      )}
    </div>
  );
}

function buildFailureSuggestions(job: JobStatus): string[] {
  if (job.lifecycle !== "failed" || !job.error) return [];

  const text = `${job.stage} ${job.error}`.toLowerCase();
  if (text.includes("apksigner") || text.includes("key-pass") || text.includes("ks-pass")) {
    return [
      "检查签名信息中的 store password、alias 和 key password 是否匹配当前签名文件。",
      "确认 Android SDK build-tools 中的 apksigner 可用，必要时在工具链区域重新探测。"
    ];
  }
  if (text.includes("jarsigner") || text.includes("bundletool")) {
    return [
      "检查 JDK 路径、jarsigner 和 bundletool 是否可用。",
      "AAB 需要使用用户密钥重新签名，并建议使用 bundletool validate/build-apks 验证。"
    ];
  }
  if (text.includes("zipalign")) {
    return [
      "确认 Android SDK build-tools 中的 zipalign 可用。",
      "检查输出目录是否存在并且当前用户有写入权限。"
    ];
  }
  if (text.includes("channels") || text.includes("channel")) {
    return [
      "多渠道打包只支持 APK，不支持 AAB。",
      "确认加固后的原始 APK 已完成签名，并且包含 APK Signature Scheme v2 签名块。"
    ];
  }
  if (text.includes("toolchain") || text.includes("not found") || text.includes("missing")) {
    return [
      "点击工具链区域的重新探测，或把 JDK、Android SDK、bundletool 放到项目 tools 目录。",
      "如果使用自定义路径，请确认路径没有填入空值或不存在的文件。"
    ];
  }
  if (text.includes("vmp")) {
    return [
      "先关闭 VMP 重试，确认基础加固链路正常。",
      "缩小 include 规则范围，排除 Application 启动链、组件生命周期入口和过大的方法。"
    ];
  }
  if (text.includes("package") || text.includes("permission") || text.includes("access")) {
    return [
      "检查输入文件是否被其他程序占用，输出目录是否有写入权限。",
      "如果输出文件已存在并被安装器或文件管理器占用，请关闭相关程序后重试。"
    ];
  }

  return ["保存任务日志后查看最后一个失败阶段；通常优先检查输入包、输出目录、工具链和签名配置。"];
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="metric">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function profileDraftToSigningConfig(draft: SigningProfileInput): SigningConfig {
  return {
    keystorePath: draft.keystorePath,
    storePassword: draft.storePassword,
    keyPassword: draft.keyPassword,
    alias: draft.alias,
    storeType: draft.storeType,
    signingScheme: draft.signingScheme ?? "v1v2"
  };
}

function splitRules(text: string) {
  return text
    .split(/[,\n]/)
    .map((item) => item.trim())
    .filter(Boolean);
}

function inferArtifactKind(path: string): "apk" | "aab" | "unknown" {
  const normalized = path.trim().toLowerCase();
  if (normalized.endsWith(".apk")) return "apk";
  if (normalized.endsWith(".aab")) return "aab";
  return "unknown";
}

function formatSigningScheme(value?: string | null) {
  return value === "v1v2v3" ? "V1+V2+V3" : "V1+V2";
}

function formatNumber(value: number) {
  return new Intl.NumberFormat("zh-CN").format(value);
}
