import type { AppConfig, ComputeBackend } from './config';

export interface AcceleratorCapabilities {
  os: string;
  arch: string;
  speechMetalModels?: string[];
  backends: { backend: ComputeBackend; compiled: boolean; devices: string[] }[];
}
export interface AcceleratorOption {
  value: ComputeBackend;
  label: string;
  description: string;
  disabled?: boolean;
}
export const acceleratorLabels: Record<ComputeBackend, string> = {
  auto: 'Automatic',
  cpu: 'CPU',
  metal: 'Metal',
  cuda: 'CUDA',
  vulkan: 'Vulkan',
  rocm: 'AMD ROCm',
};
const gpuBackends: ComputeBackend[] = ['metal', 'cuda', 'vulkan', 'rocm'];
/** Built-in speech models whose only engine is the Apple GPU (no CPU version). */
export const APPLE_GPU_ONLY_SPEECH_MODELS: readonly string[] = [
  'ibm-granite/granite-speech-5.0-470m-turboctc',
];
const appleGpuOnly = (config: AppConfig) =>
  !config.models.customAsr && APPLE_GPU_ONLY_SPEECH_MODELS.includes(config.models.asr);

export function deviceLabel(capabilities: AcceleratorCapabilities): string {
  const os =
    ({ macos: 'macOS', windows: 'Windows', linux: 'Linux' } as Record<string, string>)[
      capabilities.os
    ] ?? capabilities.os;
  return `${os} · ${capabilities.os === 'macos' && capabilities.arch === 'aarch64' ? 'Apple Silicon' : capabilities.arch}`;
}
export function acceleratorReason(
  config: AppConfig,
  capabilities: AcceleratorCapabilities | null,
  backend: ComputeBackend,
): string | null {
  if (backend === 'auto') return null;
  if (appleGpuOnly(config) && backend !== 'metal')
    return 'This speech model runs only on the Apple GPU; it has no CPU version.';
  if (backend === 'cpu') return null;
  if (!capabilities) return 'Device detection is unavailable. Choose Automatic or CPU.';
  const { os, arch } = capabilities;
  if (backend === 'metal' && (os !== 'macos' || arch !== 'aarch64'))
    return 'Metal requires an Apple Silicon Mac in this app.';
  if (backend !== 'metal' && !['linux', 'windows'].includes(os))
    return `${acceleratorLabels[backend]} is unavailable on this operating system in this app.`;
  if (config.models.customAsr) {
    if (!config.cleanupEnabled)
      return 'This speech profile runs on CPU and the writing model is off.';
  } else if (appleGpuOnly(config)) {
    if (!capabilities.speechMetalModels?.includes(config.models.asr))
      return 'This speech model is not included in this app build.';
  } else {
    if (
      [
        'nvidia/parakeet-tdt-0.6b-v3',
        'Qwen/Qwen3-ASR-0.6B',
        'CohereLabs/cohere-transcribe-03-2026',
        'openai/whisper-small',
        'openai/whisper-large-v3-turbo',
        'moonshine-ai/moonshine-base',
      ].includes(config.models.asr)
    ) {
      if (backend !== 'metal' || !capabilities.speechMetalModels?.includes(config.models.asr))
        return 'The selected speech model supports Automatic and CPU in this app build.';
      if (config.language !== 'en')
        return 'Metal speech recognition currently requires English for this model.';
    }
    if (backend === 'rocm' && arch !== 'x86_64')
      return 'This speech runtime has no ROCm/Vulkan package for this architecture.';
    if (
      (backend !== 'metal' && !['x86_64', 'aarch64'].includes(arch)) ||
      (os === 'windows' && arch !== 'x86_64')
    )
      return 'This speech runtime has no matching package for this architecture.';
  }
  const support = capabilities.backends.find((entry) => entry.backend === backend);
  if (!support?.compiled)
    return `${acceleratorLabels[backend]} support is not included in this app build.`;
  if (!support.devices.length)
    return `No usable ${acceleratorLabels[backend]} GPU was detected by this app build. Check the driver and restart the app.`;
  return null;
}
export function acceleratorOptions(
  config: AppConfig,
  capabilities: AcceleratorCapabilities | null,
): AcceleratorOption[] {
  const cpuReason = acceleratorReason(config, capabilities, 'cpu');
  const options: AcceleratorOption[] = [
    {
      value: 'auto',
      label: 'Automatic',
      description: appleGpuOnly(config)
        ? 'Use the Apple GPU'
        : 'Choose automatically, with CPU fallback',
    },
    {
      value: 'cpu',
      label: cpuReason ? 'CPU · unavailable' : 'CPU',
      description: cpuReason ?? 'Run on the processor',
      disabled: Boolean(cpuReason),
    },
  ];
  for (const backend of gpuBackends) {
    const reason = acceleratorReason(config, capabilities, backend);
    if (!reason || config.computeBackend === backend)
      options.push({
        value: backend,
        label: `${acceleratorLabels[backend]}${reason ? (capabilities ? ' · unavailable' : ' · not checked') : ''}`,
        description:
          reason ??
          capabilities!.backends.find((entry) => entry.backend === backend)!.devices.join(', '),
        disabled: Boolean(reason),
      });
  }
  return options;
}
