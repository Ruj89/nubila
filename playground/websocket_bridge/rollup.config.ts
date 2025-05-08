import typescript from "@rollup/plugin-typescript";
import terser from "@rollup/plugin-terser";
import nodeResolve from "@rollup/plugin-node-resolve";
import commonjs from "@rollup/plugin-commonjs"
import json from "@rollup/plugin-json";
import copy from "rollup-plugin-copy";
import replace from '@rollup/plugin-replace';

let common = {
  input: "src/main.ts",
  output: {
    format: "esm",
    sourcemap: true
  },
  plugins: [
    terser(),
    commonjs()
  ]
}

export default [
  {
    ...common,
    output: {
      ...common.output,
      file: "dist/node/index.js",
    },
    plugins: [
      replace({
        preventAssignment: true,
        delimiters: ['', ''],
        values: {
          'require("../../../build/Release/node_datachannel.node")': 
          'require("./node_datachannel.node")'
        }
      }),
      ...common.plugins,
      nodeResolve({
        preferBuiltins: true
      }),
      json(),
      typescript({
        tsconfig: "./tsconfig.json",
        outDir: "./dist/node"
      }),
      copy({
        targets: [
          {
            src: "node_modules/node-datachannel/build/Release/node_datachannel.node",
            dest: "./dist/node/"
          }]
      })
    ]
  }, {
    ...common,
    output: {
      ...common.output,
      file: "dist/browser/index.js",
    },
    external: ["node-datachannel", "dotenv"],
    plugins: [
      ...common.plugins,
      nodeResolve({
        browser: true
      }),
      typescript({
        tsconfig: "./tsconfig.json",
        outDir: "./dist/browser"
      }),
    ]
  }];