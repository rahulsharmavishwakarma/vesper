// Update notes for About's "What's new". Release feeds carry the CHANGELOG
// section (scripts/release-notes.mjs); older feeds carry GitHub's generated
// pull-request list. Both become short grouped lists of plain text: no
// Markdown syntax, link targets, authors or pull-request URLs.

export interface NotesSection {
  title: string;
  items: string[];
}

const GENERATED_CREDIT = /\s+by @[\w-]+ in https?:\/\/\S+$/;
const SKIPPED_SECTIONS = /^new contributors$/i;
const UNTITLED_SECTIONS = /^what[’']?s changed$/i;

function plain(text: string): string {
  return text
    .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
    .replace(/\*\*|`/g, '')
    .replace(GENERATED_CREDIT, '')
    .replace(/\s+/g, ' ')
    .trim();
}

export function parseReleaseNotes(notes: string): NotesSection[] {
  const sections: NotesSection[] = [];
  let section: NotesSection | null = null;
  let skipping = false;
  let item: string[] | null = null;
  const flush = () => {
    const text = item ? plain(item.join(' ')) : '';
    item = null;
    if (!text || skipping) return;
    if (!section) {
      section = { title: '', items: [] };
      sections.push(section);
    }
    section.items.push(text);
  };
  for (const raw of notes.split(/\r?\n/)) {
    const line = raw.trim();
    const heading = /^#{1,6}\s+(.*)$/.exec(line);
    if (heading) {
      flush();
      const title = plain(heading[1]);
      skipping = SKIPPED_SECTIONS.test(title);
      section = { title: UNTITLED_SECTIONS.test(title) ? '' : title, items: [] };
      sections.push(section);
      continue;
    }
    const bullet = /^[-*+]\s+(.*)$/.exec(line);
    if (bullet) {
      flush();
      item = [bullet[1]];
    } else if (!line || line.startsWith('<!--') || /^\*\*Full Changelog\*\*/i.test(line)) {
      flush();
    } else if (item) {
      item.push(line);
    } else {
      item = [line];
    }
  }
  flush();
  return sections.filter((entry) => entry.items.length > 0);
}
