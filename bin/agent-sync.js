#!/usr/bin/env node
import fs from 'node:fs/promises';
import { runDoctor, renderDoctorReport } from '../src/doctor.js';
import { runDiff, renderDiffReport } from '../src/diff.js';
import { runPlan, renderPlanReport } from '../src/plan.js';

function usage(exitCode = 0) {
  const out = exitCode === 0 ? console.log : console.error;
  out('agent-sync\n\nUsage:\n  agent-sync doctor [--json] [--home PATH] [--project PATH] [--output PATH] [--max-depth N] [--max-entries N]\n  agent-sync diff --from FROM.json --to TO.json [--json] [--output PATH]\n  agent-sync plan --from FROM.json --to TO.json [--target windows|macos|linux] [--json] [--output PATH]\n  agent-sync --help\n');
  process.exit(exitCode);
}

function optionName(key) {
  return key.replace(/[A-Z]/g, (match) => '-' + match.toLowerCase());
}

function parseArgs(argv) {
  const args = [...argv];
  const command = args.shift();
  if (!command || command === '--help' || command === '-h') usage(0);
  if (!['doctor', 'diff', 'plan'].includes(command)) {
    console.error('Unknown command: ' + command);
    usage(1);
  }

  const options = { command, json: false };
  while (args.length) {
    const arg = args.shift();
    if (arg === '--json') {
      options.json = true;
    } else if (arg === '--home') {
      options.home = args.shift();
    } else if (arg === '--project') {
      options.project = args.shift();
    } else if (arg === '--output') {
      options.output = args.shift();
    } else if (arg === '--from') {
      options.from = args.shift();
    } else if (arg === '--to') {
      options.to = args.shift();
    } else if (arg === '--target') {
      options.target = args.shift();
    } else if (arg === '--max-depth') {
      options.maxDepth = Number(args.shift());
    } else if (arg === '--max-entries') {
      options.maxEntries = Number(args.shift());
    } else if (arg === '--help' || arg === '-h') {
      usage(0);
    } else {
      console.error('Unknown option: ' + arg);
      usage(1);
    }
  }

  for (const key of ['home', 'project', 'output', 'from', 'to', 'target']) {
    if (options[key] === undefined) continue;
    if (!options[key]) throw new Error('Missing value for --' + key);
  }
  for (const key of ['maxDepth', 'maxEntries']) {
    if (options[key] === undefined) continue;
    if (!Number.isFinite(options[key]) || options[key] < 0) {
      throw new Error('Invalid numeric value for --' + optionName(key));
    }
  }
  if ((command === 'diff' || command === 'plan') && (!options.from || !options.to)) {
    throw new Error(command + ' requires --from and --to manifest paths');
  }
  return options;
}

async function readJsonFile(filePath) {
  return JSON.parse(await fs.readFile(filePath, 'utf8'));
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  let result;
  let output;
  if (options.command === 'doctor') {
    result = await runDoctor(options);
    output = options.json ? JSON.stringify(result, null, 2) + '\n' : renderDoctorReport(result) + '\n';
  } else if (options.command === 'diff') {
    const fromManifest = await readJsonFile(options.from);
    const toManifest = await readJsonFile(options.to);
    result = runDiff(fromManifest, toManifest, { fromLabel: options.from, toLabel: options.to });
    output = options.json ? JSON.stringify(result, null, 2) + '\n' : renderDiffReport(result) + '\n';
  } else if (options.command === 'plan') {
    const fromManifest = await readJsonFile(options.from);
    const toManifest = await readJsonFile(options.to);
    result = runPlan(fromManifest, toManifest, { fromLabel: options.from, toLabel: options.to, target: options.target });
    output = options.json ? JSON.stringify(result, null, 2) + '\n' : renderPlanReport(result) + '\n';
  }

  if (options.output) {
    await fs.writeFile(options.output, output);
  } else {
    process.stdout.write(output);
  }
}

main().catch((error) => {
  console.error('agent-sync: ' + error.message);
  process.exit(1);
});
