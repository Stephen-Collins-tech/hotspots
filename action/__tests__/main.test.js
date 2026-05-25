jest.mock('@actions/core', () => ({
  info: jest.fn(),
  warning: jest.fn(),
  setFailed: jest.fn(),
  setOutput: jest.fn(),
  getInput: jest.fn(() => ''),
  getBooleanInput: jest.fn(() => false),
  summary: { addRaw: jest.fn().mockReturnThis(), write: jest.fn() }
}));
jest.mock('@actions/exec', () => ({ exec: jest.fn() }));
jest.mock('@actions/github', () => ({
  context: { eventName: 'push', repo: { owner: 'owner', repo: 'repo' }, issue: { number: 1 } },
  getOctokit: jest.fn()
}));
jest.mock('@actions/tool-cache', () => ({
  find: jest.fn(() => ''),
  downloadTool: jest.fn(),
  extractZip: jest.fn(),
  extractTar: jest.fn(),
  cacheDir: jest.fn()
}));

const fs = require('fs');
const os = require('os');
const path = require('path');
const { execFileSync } = require('child_process');

const actionRoot = path.resolve(__dirname, '..');
const buildRoot = path.join(actionRoot, '.jest-build');

function loadMain() {
  fs.rmSync(buildRoot, { recursive: true, force: true });
  execFileSync(
    path.join(actionRoot, 'node_modules', '.bin', 'tsc'),
    ['--outDir', buildRoot, '--declaration', 'false', '--declarationMap', 'false', '--sourceMap', 'false'],
    { cwd: actionRoot, stdio: 'pipe' }
  );
  delete require.cache[require.resolve(path.join(buildRoot, 'main.js'))];
  return require(path.join(buildRoot, 'main.js'));
}

describe('action binary resolution helpers', () => {
  let main;
  let tempRoot;

  beforeEach(() => {
    jest.restoreAllMocks();
    main = loadMain();
    tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'hotspots-action-test-'));
  });

  afterEach(() => {
    jest.restoreAllMocks();
    if (tempRoot) fs.rmSync(tempRoot, { recursive: true, force: true });
  });

  test('maps linux x64 runners to release asset naming', () => {
    jest.spyOn(os, 'platform').mockReturnValue('linux');
    jest.spyOn(os, 'arch').mockReturnValue('x64');

    expect(main.getPlatformInfo()).toEqual({
      platform: 'linux',
      arch: 'x86_64',
      ext: 'tar.gz',
      binaryName: 'hotspots'
    });
  });

  test('maps windows x64 runners to zip and exe naming', () => {
    jest.spyOn(os, 'platform').mockReturnValue('win32');
    jest.spyOn(os, 'arch').mockReturnValue('x64');

    expect(main.getPlatformInfo()).toEqual({
      platform: 'windows',
      arch: 'x86_64',
      ext: 'zip',
      binaryName: 'hotspots.exe'
    });
  });

  test('uses configured binary path before cached or downloaded binaries', async () => {
    jest.spyOn(os, 'platform').mockReturnValue('linux');
    const configured = path.join(tempRoot, 'custom-hotspots');
    fs.writeFileSync(configured, 'fake-binary');

    await expect(main.installFaultline('latest', undefined, configured)).resolves.toBe(configured);
    expect(fs.statSync(configured).mode & 0o111).not.toBe(0);
  });

  test('rejects missing configured binary path', async () => {
    const missing = path.join(tempRoot, 'missing-hotspots');

    await expect(main.installFaultline('latest', undefined, missing)).rejects.toThrow(
      /Configured hotspots binary does not exist/
    );
  });
});
