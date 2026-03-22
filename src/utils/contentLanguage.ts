const HAN_SCRIPT_PATTERN = /\p{Script=Han}/u;
const HIRAGANA_SCRIPT_PATTERN = /\p{Script=Hiragana}/u;
const KATAKANA_SCRIPT_PATTERN = /\p{Script=Katakana}/u;
const HANGUL_SCRIPT_PATTERN = /\p{Script=Hangul}/u;

function normalizeLanguageTag(value?: string | null) {
  const trimmed = value?.trim();
  return trimmed ? trimmed : undefined;
}

export function getPreferredLanguageTag(
  navigatorLike?: Pick<Navigator, "language"> | null,
) {
  return normalizeLanguageTag(navigatorLike?.language) ?? "en";
}

export function detectContentLanguageTag(
  text: string,
  fallbackLanguage?: string | null,
) {
  const fallback = normalizeLanguageTag(fallbackLanguage);
  if (!text.trim()) {
    return undefined;
  }
  if (HANGUL_SCRIPT_PATTERN.test(text)) {
    return "ko";
  }
  if (
    HIRAGANA_SCRIPT_PATTERN.test(text) ||
    KATAKANA_SCRIPT_PATTERN.test(text)
  ) {
    return fallback?.toLowerCase().startsWith("ja") ? fallback : "ja";
  }
  if (HAN_SCRIPT_PATTERN.test(text)) {
    if (fallback?.toLowerCase().startsWith("zh")) {
      return fallback;
    }
    if (fallback?.toLowerCase().startsWith("ja")) {
      return fallback;
    }
    return "zh-CN";
  }
  return undefined;
}
