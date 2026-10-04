import type { CaptureDesign } from './config';

// Content dimensions. Native windows add 12px around these for focus and shadow.
export const captureDesignOptions: {
  value: CaptureDesign;
  label: string;
  description: string;
  width: number;
}[] = [
  {
    value: 'capsule',
    label: 'Capsule',
    description: 'One slim pill with waveform and actions',
    width: 400,
  },
  {
    value: 'split',
    label: 'Split',
    description: 'Glass card with a separate action pill · default',
    width: 430,
  },
  {
    value: 'island',
    label: 'Island',
    description: 'Compact pill that grows with the transcript',
    width: 400,
  },
];
