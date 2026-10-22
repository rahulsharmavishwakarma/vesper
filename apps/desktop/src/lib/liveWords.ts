/** One word of a live draft, keyed by its position and text. A word that stays
 * the same between drafts keeps its key, so the screen keeps it still; a new or
 * revised word gets a new key and can ease in. When a grey (provisional) word
 * becomes committed at the same place, only its colour changes. */
export interface LiveWord {
  key: string;
  text: string;
  provisional: boolean;
}

const split = (value: string) => value.split(/\s+/).filter(Boolean);

export function liveWords(committed: string, provisional: string): LiveWord[] {
  const words = [
    ...split(committed).map((text) => ({ text, provisional: false })),
    ...split(provisional).map((text) => ({ text, provisional: true })),
  ];
  return words.map((word, index) => ({ ...word, key: `${index}:${word.text}` }));
}
