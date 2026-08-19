// Letter -> note mapping. Swap this file to change the tuning of the piece.
// Chromatic: A = C3, B = C#3, C = D3 … Z = 25 semitones above C3.
const LETTERS = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ';
const BASE_MIDI = 48; // C3

function letterToMidi(letter) {
  const step = LETTERS.indexOf(String(letter).toUpperCase());
  return step < 0 ? null : BASE_MIDI + step;
}

function midiToFreq(midi) {
  return 440 * Math.pow(2, (midi - 69) / 12);
}

function letterToFreq(letter) {
  const midi = letterToMidi(letter);
  return midi === null ? null : midiToFreq(midi);
}

// Display label, e.g. "C3" / "C#3" — shown under each cell.
function letterToNoteName(letter) {
  const midi = letterToMidi(letter);
  if (midi === null) return '';
  const names = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];
  return names[midi % 12] + (Math.floor(midi / 12) - 1);
}
