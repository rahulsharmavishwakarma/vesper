import type { CaptureDesign } from './config';

// Content dimensions. Native windows add 12px around these for focus and shadow.
export const captureDesignOptions: {
  value: CaptureDesign;
  label: string;
  description: string;
  width: number;
}[] = [
  {
    value: 'split',
    label: 'Split',
    description: 'Separate waveform and action pod · default',
    width: 372,
  },
  { value: 'ribbon', label: 'Ribbon', description: 'Everything in one slim strip', width: 364 },
  {
    value: 'underline',
    label: 'Underline',
    description: 'Live words above the waveform',
    width: 430,
  },
  {
    value: 'studio',
    label: 'Studio',
    description: 'Wide waveform with labeled controls',
    width: 388,
  },
  {
    value: 'margin',
    label: 'Margin',
    description: 'Vertical controls with a side status label',
    width: 248,
  },
  { value: 'counter', label: 'Counter', description: 'Elapsed time takes the lead', width: 176 },
];
