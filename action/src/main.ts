import * as core from '@actions/core';
import * as exec from '@actions/exec';
import * as github from '@actions/github';
import * as tc from '@actions/tool-cache';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

interface FaultlineInputs {
  path: string;
  policy: string;
  minLrs?: string;
  config?: string;
  failOn: string;
  version: string;
  githubToken: string;
  postComment: boolean;
}

interface FaultlineResult {
  violations: any[];
  passed: boolean;
  summary: string;
  reportPath?: string;
}

async function getInputs(): Promise<FaultlineInputs> {
  return {
    path: core.getInput('path') || '.',
    policy: core.getInput('policy') || 'critical-introduction',
    minLrs: core.getInput('min-lrs') || undefined,
    config: core.getInput('config') || undefined,
    failOn: core.getInput('fail-on') || 'error',
    version: core.getInput('version') || 'latest',
    githubToken: core.getInput('github-token'),
    postComment: core.getBooleanInput('post-comment')
  };
}

function getPlatformInfo(): { platform: string; arch: string; ext: string; binaryName: string } {
  const nodePlatform = os.platform();
  const nodeArch = os.arch();

  const platformMap: Record<string, string> = {
    linux: 'linux',
    darwin: 'darwin',
    win32: 'windows'
  };

  const archMap: Record<string, string> = {
    x64: 'x86_64',
    arm64: 'aarch64'
  };

  const platform = platformMap[nodePlatform];
  const arch = archMap[nodeArch];

  if (!platform) throw new Error(`Unsupported platform: ${nodePlatform}`);
  if (!arch) throw new Error(`Unsupported architecture: ${nodeArch}`);

  const ext = nodePlatform === 'win32' ? 'zip' : 'tar.gz';
  const binaryName = nodePlatform === 'win32' ? 'hotspots.exe' : 'hotspots';

  return { platform, arch, ext, binaryName };
}

async function resolveVersion(version: string, token?: string): Promise<string> {
  if (version !== 'latest') {
    return version;
  }

  core.info('Resolving latest hotspots release from GitHub API...');

  const headers: Record<string, string> = {
    Accept: 'application/vnd.github.v3+json',
    'User-Agent': 'hotspots-action'
  };
  if (token) {
    headers['Authorization'] = `token ${token}`;
  }

  const response = await fetch('https://api.github.com/repos/Stephen-Collins-tech/hotspots/releases/latest', { headers });
  if (!response.ok) {
    throw new Error(`Failed to resolve latest version: ${response.status} ${response.statusText}`);
  }

  const data = (await response.json()) as { tag_name: string };
  const resolved = data.tag_name.replace(/^v/, '');
  core.info(`Resolved latest version: ${resolved}`);
  return resolved;
}

async function installFaultline(version: string, token?: string): Promise<string> {
  core.info(`Installing hotspots version: ${version}`);

  const resolvedVersion = await resolveVersion(version, token);

  // Check if already cached
  const cachedPath = tc.find('hotspots', resolvedVersion);
  if (cachedPath) {
    core.info(`Found cached hotspots at ${cachedPath}`);
    const { binaryName } = getPlatformInfo();
    return path.join(cachedPath, binaryName);
  }

  const { platform, arch, ext, binaryName } = getPlatformInfo();
  const downloadUrl = `https://github.com/Stephen-Collins-tech/hotspots/releases/download/v${resolvedVersion}/hotspots-${platform}-${arch}.${ext}`;

  core.info(`Downloading hotspots from ${downloadUrl}`);

  try {
    const downloadPath = await tc.downloadTool(downloadUrl);
    const extractPath = ext === 'zip'
      ? await tc.extractZip(downloadPath)
      : await tc.extractTar(downloadPath);
    const cachedDir = await tc.cacheDir(extractPath, 'hotspots', resolvedVersion);

    const binaryPath = path.join(cachedDir, binaryName);

    if (os.platform() !== 'win32') {
      await exec.exec('chmod', ['+x', binaryPath]);
    }

    core.info(`Hotspots installed successfully at ${binaryPath}`);
    return binaryPath;
  } catch (error) {
    core.warning(`Failed to download prebuilt binary: ${error}`);
    core.info('Attempting to build from source...');
    return await buildFromSource();
  }
}

async function buildFromSource(): Promise<string> {
  core.info('Building hotspots from source using cargo...');

  // Check if we're in the hotspots repo (for local development/testing)
  const repoRoot = process.env.GITHUB_WORKSPACE || process.cwd();
  const cargoToml = path.join(repoRoot, 'Cargo.toml');

  if (fs.existsSync(cargoToml)) {
    core.info('Found Cargo.toml in workspace, building local version...');
    await exec.exec('cargo', ['build', '--release', '--bin', 'hotspots']);
    return path.join(repoRoot, 'target', 'release', 'hotspots');
  }

  throw new Error('Could not download binary and no local source found');
}

async function detectContext(): Promise<'pr' | 'push'> {
  const eventName = github.context.eventName;

  if (eventName === 'pull_request' || eventName === 'pull_request_target') {
    return 'pr';
  }

  return 'push';
}

async function runFaultline(
  binaryPath: string,
  inputs: FaultlineInputs,
  context: 'pr' | 'push'
): Promise<FaultlineResult> {
  const args: string[] = ['analyze'];

  // Determine mode based on context
  if (context === 'pr') {
    args.push('--mode', 'delta');
    // Note: hotspots automatically detects merge-base from git context in PR mode
  } else {
    args.push('--mode', 'snapshot');
  }

  if (inputs.policy && context === 'pr') {
    args.push('--policy', inputs.policy);
  }

  // Add optional parameters
  if (inputs.minLrs) {
    args.push('--min-lrs', inputs.minLrs);
  }

  if (inputs.config) {
    args.push('--config', inputs.config);
  }

  // Always generate JSON output for parsing
  args.push('--format', 'json');

  // Add path as positional argument (must be last)
  args.push(inputs.path);

  // Also generate HTML report
  const reportPath = path.join(process.env.GITHUB_WORKSPACE || '.', 'hotspots-report.html');

  core.info(`Running: ${binaryPath} ${args.join(' ')}`);

  let stdout = '';
  let stderr = '';

  const exitCode = await exec.exec(binaryPath, args, {
    listeners: {
      stdout: (data: Buffer) => {
        stdout += data.toString();
      },
      stderr: (data: Buffer) => {
        stderr += data.toString();
      }
    },
    ignoreReturnCode: true
  });

  // Also generate HTML report
  await exec.exec(binaryPath, [...args.filter(a => a !== '--format' && a !== 'json'), '--format', 'html', '--output', reportPath], {
    ignoreReturnCode: true
  });

  core.info(`Faultline exited with code ${exitCode}`);

  if (stderr) {
    core.warning(`Stderr: ${stderr}`);
  }

  // Parse JSON output
  let result: any;
  try {
    result = JSON.parse(stdout);
  } catch (error) {
    core.error(`Failed to parse hotspots output: ${error}`);
    core.error(`Raw output: ${stdout}`);
    throw new Error('Failed to parse hotspots output');
  }

  // Determine if passed based on policy violations and fail-on setting
  const violations = result.violations || [];
  const errors = violations.filter((v: any) => v.level === 'error');
  const warnings = violations.filter((v: any) => v.level === 'warning');

  let passed = true;
  if (inputs.failOn === 'error' && errors.length > 0) {
    passed = false;
  } else if (inputs.failOn === 'warn' && (errors.length > 0 || warnings.length > 0)) {
    passed = false;
  }

  // Generate summary
  const summary = generateSummary(result, context);

  return {
    violations,
    passed,
    summary,
    reportPath: fs.existsSync(reportPath) ? reportPath : undefined
  };
}

function generateSummary(result: any, context: 'pr' | 'push'): string {
  const violations = result.violations || [];
  const errors = violations.filter((v: any) => v.level === 'error');
  const warnings = violations.filter((v: any) => v.level === 'warning');
  const infos = violations.filter((v: any) => v.level === 'info');

  let summary = '# Hotspots Analysis Results\n\n';

  if (context === 'pr') {
    summary += '**Mode:** Delta (PR analysis)\n\n';
  } else {
    summary += '**Mode:** Snapshot (baseline analysis)\n\n';
  }

  if (violations.length === 0) {
    summary += '✅ **No violations found!**\n\n';
    summary += 'All functions meet complexity thresholds.\n';
    return summary;
  }

  summary += `**Summary:** ${errors.length} error(s), ${warnings.length} warning(s), ${infos.length} info\n\n`;

  if (errors.length > 0) {
    summary += '## ❌ Errors\n\n';
    summary += '| Function | File | LRS | Policy |\n';
    summary += '|----------|------|-----|--------|\n';
    errors.slice(0, 10).forEach((v: any) => {
      summary += `| ${v.function_name} | ${v.file}:${v.line} | ${v.lrs.toFixed(1)} | ${v.policy} |\n`;
    });
    if (errors.length > 10) {
      summary += `\n*...and ${errors.length - 10} more errors*\n`;
    }
    summary += '\n';
  }

  if (warnings.length > 0) {
    summary += '## ⚠️ Warnings\n\n';
    summary += '| Function | File | LRS | Policy |\n';
    summary += '|----------|------|-----|--------|\n';
    warnings.slice(0, 10).forEach((v: any) => {
      summary += `| ${v.function_name} | ${v.file}:${v.line} | ${v.lrs.toFixed(1)} | ${v.policy} |\n`;
    });
    if (warnings.length > 10) {
      summary += `\n*...and ${warnings.length - 10} more warnings*\n`;
    }
    summary += '\n';
  }

  if (infos.length > 0) {
    summary += '## 👀 Watch\n\n';
    summary += `${infos.length} function(s) approaching thresholds\n\n`;
  }

  return summary;
}

async function postPRComment(
  token: string,
  summary: string,
  reportPath?: string
): Promise<void> {
  if (!github.context.payload.pull_request) {
    core.info('Not a PR context, skipping comment');
    return;
  }

  const octokit = github.getOctokit(token);
  const { owner, repo } = github.context.repo;
  const prNumber = github.context.payload.pull_request.number;

  let body = summary;

  if (reportPath) {
    body += '\n\n---\n';
    body += '*📊 Full HTML report available in workflow artifacts*\n';
  }

  // Check if we already have a comment
  const comments = await octokit.rest.issues.listComments({
    owner,
    repo,
    issue_number: prNumber
  });

  const botComment = comments.data.find(
    comment => comment.user?.type === 'Bot' && comment.body?.includes('Hotspots Analysis Results')
  );

  if (botComment) {
    // Update existing comment
    await octokit.rest.issues.updateComment({
      owner,
      repo,
      comment_id: botComment.id,
      body
    });
    core.info('Updated existing PR comment');
  } else {
    // Create new comment
    await octokit.rest.issues.createComment({
      owner,
      repo,
      issue_number: prNumber,
      body
    });
    core.info('Created new PR comment');
  }
}

async function run(): Promise<void> {
  try {
    const inputs = await getInputs();

    core.info('Starting Faultline analysis...');
    core.info(`Inputs: ${JSON.stringify(inputs, null, 2)}`);

    // Install hotspots
    const binaryPath = await installFaultline(inputs.version, inputs.githubToken);

    // Detect context (PR or push)
    const context = await detectContext();
    core.info(`Detected context: ${context}`);

    // Run hotspots
    const result = await runFaultline(binaryPath, inputs, context);

    // Set outputs
    core.setOutput('violations', JSON.stringify(result.violations));
    core.setOutput('passed', result.passed.toString());
    core.setOutput('summary', result.summary);
    if (result.reportPath) {
      core.setOutput('report-path', result.reportPath);
    }

    // Write job summary
    await core.summary
      .addRaw(result.summary)
      .write();

    // Post PR comment if requested
    if (inputs.postComment && context === 'pr' && inputs.githubToken) {
      await postPRComment(inputs.githubToken, result.summary, result.reportPath);
    }

    // Fail if needed
    if (!result.passed) {
      core.setFailed(`Hotspots analysis failed: ${result.violations.length} violation(s)`);
    } else {
      core.info('✅ Faultline analysis passed!');
    }

  } catch (error) {
    if (error instanceof Error) {
      core.setFailed(error.message);
    } else {
      core.setFailed('Unknown error occurred');
    }
  }
}

run();
