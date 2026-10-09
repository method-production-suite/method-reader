// The typewriter's sounds for the page that writes itself (spec 2026-10-01,
// decision 20): made in the window with Web Audio, so there is no recorded
// file to ship or license. A key is a few milliseconds of filtered noise; the
// carriage return is a short bell. page-timing.js decides when each is due,
// and page-show.js plays them only when "Typewriter sounds" is on.
//
// Every call is safe to make anywhere: a window with no Web Audio, or one
// that refuses to start it, is simply quiet.

/** How loud a key is, and how long it lasts (seconds). */
const KEY = { gain: 0.3, length: 0.035 };
/** The bell's pitch (Hz), loudness and fade (seconds). */
const DING = { pitch: 2093, gain: 0.08, length: 0.5 };

/** The sounds, made with `Audio` (an AudioContext constructor; the
 *  window's own by default). Nothing is created until the first sound. */
export function createTypewriter(Audio = globalThis.AudioContext ?? globalThis.webkitAudioContext) {
  let audio = null;
  let noise = null;
  let broken = typeof Audio !== 'function';

  function ready() {
    if (broken) return null;
    if (audio !== null) return audio;
    try {
      audio = new Audio();
      const frames = Math.ceil(audio.sampleRate * KEY.length);
      noise = audio.createBuffer(1, frames, audio.sampleRate);
      const data = noise.getChannelData(0);
      for (let i = 0; i < frames; i += 1) data[i] = Math.random() * 2 - 1;
    } catch {
      broken = true;
      audio = null;
      return null;
    }
    return audio;
  }

  /** A fade from `level` to nothing over `length` seconds, starting now. */
  function envelope(at, level, length) {
    const gain = audio.createGain();
    gain.gain.setValueAtTime(level, at);
    gain.gain.exponentialRampToValueAtTime(0.0001, at + length);
    gain.connect(audio.destination);
    return gain;
  }

  return {
    key() {
      if (ready() === null) return;
      try {
        const at = audio.currentTime;
        const source = audio.createBufferSource();
        source.buffer = noise;
        const filter = audio.createBiquadFilter();
        filter.type = 'bandpass';
        // A little different every time, the way no two keys sound alike.
        filter.frequency.value = 1800 + Math.random() * 1600;
        filter.Q.value = 1.4;
        source.connect(filter);
        filter.connect(envelope(at, KEY.gain, KEY.length));
        source.start(at);
        source.stop(at + KEY.length);
      } catch {
        // A sound is never worth an error.
      }
    },
    ding() {
      if (ready() === null) return;
      try {
        const at = audio.currentTime;
        const bell = audio.createOscillator();
        bell.type = 'sine';
        bell.frequency.value = DING.pitch;
        bell.connect(envelope(at, DING.gain, DING.length));
        bell.start(at);
        bell.stop(at + DING.length);
      } catch {
        // Quiet, as above.
      }
    },
    close() {
      try {
        audio?.close?.();
      } catch {
        // Already closed.
      }
      audio = null;
    },
  };
}
