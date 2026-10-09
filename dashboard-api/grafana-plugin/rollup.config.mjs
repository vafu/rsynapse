import esbuild from 'rollup-plugin-esbuild';
const make = (input, file) => ({
  input,
  output: { file, format: 'system', sourcemap: true },
  external: ['react', '@grafana/data', '@grafana/runtime'],
  plugins: [esbuild({ jsx: 'transform', target: 'es2020' })],
});
export default [make('src/module.tsx','dist/module.js'),make('src/workday-module.tsx','dist-workday/module.js')];
