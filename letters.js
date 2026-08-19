// Letter -> note mapping. Swap this file to change the tuning of the piece.
// Chromatic: A = C3, B = C#3, C = D3 … Z = 25 semitones above C3.
// octave = display-side transpose in whole octaves (0 = as written above).
const LETTERS = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ';
const BASE_MIDI = 48; // C3

function letterToMidi(letter, octave = 0) {
  const step = LETTERS.indexOf(String(letter).toUpperCase());
  return step < 0 ? null : BASE_MIDI + step + octave * 12;
}

function midiToFreq(midi) {
  return 440 * Math.pow(2, (midi - 69) / 12);
}

function letterToFreq(letter, octave = 0) {
  const midi = letterToMidi(letter, octave);
  return midi === null ? null : midiToFreq(midi);
}

// Display label, e.g. "C3" / "C#3" — shown under each cell.
function letterToNoteName(letter, octave = 0) {
  const midi = letterToMidi(letter, octave);
  if (midi === null) return '';
  const names = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];
  return names[midi % 12] + (Math.floor(midi / 12) - 1);
}
