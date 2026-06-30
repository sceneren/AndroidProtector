export type ArtifactKind = "apk" | "aab" | "unknown";

export interface ToolStatus {
  available: boolean;
  path?: string | null;
  version?: string | null;
}

export interface BuildToolInfo {
  version: string;
  path: string;
  zipalign?: string | null;
  apksigner?: string | null;
  aapt2?: string | null;
}

export interface ToolchainStatus {
  ok: boolean;
  javaHome?: string | null;
  androidSdk?: string | null;
  java: ToolStatus;
  javac: ToolStatus;
  jarsigner: ToolStatus;
  bundletool: ToolStatus;
  buildTools: BuildToolInfo[];
  selectedBuildTools?: BuildToolInfo | null;
  zipalign: ToolStatus;
  apksigner: ToolStatus;
  issues: string[];
}

export interface ToolchainPaths {
  javaHome?: string | null;
  androidSdk?: string | null;
  buildToolsDir?: string | null;
  zipalign?: string | null;
  apksigner?: string | null;
  jarsigner?: string | null;
  bundletool?: string | null;
}

export interface DexFileInfo {
  name: string;
  sizeBytes: number;
  methodCount: number;
  classCount: number;
  virtualizableMethods: number;
}

export interface ArtifactInfo {
  path: string;
  fileName: string;
  kind: ArtifactKind;
  sizeBytes: number;
  packageName?: string | null;
  versionName?: string | null;
  versionCode?: string | null;
  applicationClass?: string | null;
  minSdk?: string | null;
  targetSdk?: string | null;
  dexFiles: DexFileInfo[];
  nativeAbis: string[];
  signed: boolean;
  signatureSchemes: string[];
  entryCount: number;
  warnings: string[];
}

export interface VmpOptions {
  enabled: boolean;
  includeRules: string[];
  excludeRules: string[];
  maxMethodSize: number;
  abiSelection: string[];
  unsupportedMethodPolicy: string;
}

export interface ProtectionOptions {
  dexEncryption: boolean;
  antiDebug: boolean;
  signatureTamperCheck: boolean;
  legacyApiFallback: boolean;
}

export interface SigningConfig {
  keystorePath: string;
  storePassword: string;
  keyPassword?: string | null;
  alias: string;
  storeType?: string | null;
}

export interface SigningProfile {
  id: string;
  name: string;
  keystorePath: string;
  alias: string;
  storeType?: string | null;
  certificateSummary?: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface SigningProfileInput {
  id?: string | null;
  name: string;
  keystorePath: string;
  storePassword: string;
  keyPassword?: string | null;
  alias: string;
  storeType?: string | null;
}

export interface SigningAliasInfo {
  alias: string;
  entryType?: string | null;
  certificateSummary?: string | null;
}

export interface SigningAliasInspection {
  valid: boolean;
  storeType?: string | null;
  aliases: SigningAliasInfo[];
  issues: string[];
}

export interface AppPreferences {
  signingProfiles: SigningProfile[];
  lastOutputDir?: string | null;
  selectedSigningProfileId?: string | null;
}

export interface ProtectRequest {
  inputPath: string;
  outputDir: string;
  artifactKind?: ArtifactKind | null;
  vmpOptions: VmpOptions;
  protectionOptions: ProtectionOptions;
  signingConfig?: SigningConfig | null;
  signingProfileId?: string | null;
  toolchainPaths?: ToolchainPaths | null;
}

export interface SkipReason {
  reason: string;
  count: number;
  examples: string[];
}

export interface VmpPlan {
  enabled: boolean;
  candidateMethods: number;
  virtualizedMethods: number;
  skippedMethods: number;
  skippedReasons: SkipReason[];
  riskLevel: string;
  notes: string[];
}

export interface SigningValidation {
  valid: boolean;
  aliasFound: boolean;
  certificateSummary?: string | null;
  issues: string[];
}

export type JobLifecycle = "queued" | "running" | "succeeded" | "failed" | "canceled";

export interface JobLogEntry {
  timestamp: string;
  stage: string;
  message: string;
}

export interface JobStatus {
  id: string;
  lifecycle: JobLifecycle;
  stage: string;
  progress: number;
  logs: JobLogEntry[];
  outputPath?: string | null;
  error?: string | null;
  startedAt?: string | null;
  finishedAt?: string | null;
}
