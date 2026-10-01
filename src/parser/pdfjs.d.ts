// pdfjs-dist ships types for the package root but not the build subpaths.
declare module 'pdfjs-dist/build/pdf.mjs' {
  export * from 'pdfjs-dist';
}

declare module 'pdfjs-dist/build/pdf.worker.mjs' {
  export const WorkerMessageHandler: unknown;
}

// The three decoder files pdfjs-wasm.ts embeds. Imported `with { type: "file" }`,
// which is Bun's asset form: the default export is a PATH to the file at
// runtime, not its contents, and that is true both in a dev checkout (the real
// node_modules path) and inside a compiled binary (a /$bunfs/root one). Typed
// one by one rather than as `*.wasm` / `*.js` wildcards, so that importing some
// OTHER package's file by mistake is still a compile error.
declare module 'pdfjs-dist/wasm/openjpeg_nowasm_fallback.js' {
  const path: string;
  export default path;
}
declare module 'pdfjs-dist/wasm/jbig2.wasm' {
  const path: string;
  export default path;
}
declare module 'pdfjs-dist/wasm/jbig2_nowasm_fallback.js' {
  const path: string;
  export default path;
}
