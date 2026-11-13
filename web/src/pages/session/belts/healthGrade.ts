/**
 * Shake&Tune's health text without the words the "Mechanical health" label already says. The core
 * keeps the exact upstream strings, so the shortening lives here.
 */
const GRADES: Record<string, string> = {
  "Excellent mechanical health": "Excellent",
  "Good mechanical health": "Good",
  "Acceptable mechanical health": "Acceptable",
  "Potential signs of a mechanical issue": "Potential signs of an issue",
  "Likely a mechanical issue": "Likely an issue",
  "Mechanical issue detected": "Issue detected",
  "Unknown mechanical health": "Unknown",
};

/** The grade for a health text; text this does not know is returned unchanged. */
export function healthGrade(text: string): string {
  return Object.hasOwn(GRADES, text) ? GRADES[text] : text;
}
