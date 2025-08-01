import { promises as fs } from 'fs';
import { join } from 'path';
import { platform, arch } from 'os';
import fetch from 'node-fetch';

interface PackageInfo {
  name: string;
  version: string;
  repository?: {
    url: string;
  };
}

async function getPackageInfo(): Promise<PackageInfo> {
  const packageJsonPath = join(__dirname, '..', 'package.json');
  const packageJson = await fs.readFile(packageJsonPath, 'utf-8');
  return JSON.parse(packageJson);
}

function getPlatformInfo(): { platform: string; arch: string; extension: string } {
  const platformMap: Record<string, string> = {
    'darwin': 'macos',
    'linux': 'linux',
    'win32': 'windows'
  };
  
  const archMap: Record<string, string> = {
    'x64': 'x64',
    'arm64': 'arm64'
  };
  
  const currentPlatform = platformMap[platform()] || platform();
  const currentArch = archMap[arch()] || arch();
  const extension = platform() === 'win32' ? '.exe' : '';
  
  return {
    platform: currentPlatform,
    arch: currentArch,
    extension
  };
}

function extractRepoInfo(repoUrl: string): { owner: string; repo: string } {
  const match = repoUrl.match(/github\.com[\/:]([^\/]+)\/([^\/]+?)(?:\.git)?$/);
  if (!match) {
    throw new Error(`Cannot parse GitHub repository URL: ${repoUrl}`);
  }
  return { owner: match[1], repo: match[2] };
}

async function downloadBinary(
  owner: string,
  repo: string,
  version: string,
  binaryName: string,
  platformInfo: ReturnType<typeof getPlatformInfo>,
  targetPath: string
): Promise<void> {
  const fileName = `${binaryName}-${platformInfo.platform}-${platformInfo.arch}${platformInfo.extension}`;
  const downloadUrl = `https://github.com/${owner}/${repo}/releases/download/v${version}/${fileName}`;
  
  console.log(`Downloading binary from: ${downloadUrl}`);
  
  const response = await fetch(downloadUrl);
  if (!response.ok) {
    throw new Error(`Failed to download binary: ${response.status} ${response.statusText}`);
  }
  
  const buffer = await response.buffer();
  await fs.writeFile(targetPath, buffer, { mode: 0o755 });
  
  console.log(`Binary installed successfully: ${targetPath}`);
}

async function install(): Promise<void> {
  try {
    const packageInfo = await getPackageInfo();
    const platformInfo = getPlatformInfo();
    
    if (!packageInfo.repository?.url) {
      throw new Error('Repository URL not found in package.json');
    }
    
    const { owner, repo } = extractRepoInfo(packageInfo.repository.url);
    const binaryName = 'pm';
    
    // Ensure bin directory exists
    const binDir = join(__dirname, '..', 'bin');
    await fs.mkdir(binDir, { recursive: true });
    
    const targetPath = join(binDir, binaryName + platformInfo.extension);
    
    // Check if binary already exists
    try {
      await fs.access(targetPath);
      console.log('Binary already exists, skipping download');
      return;
    } catch {
      // Binary doesn't exist, proceed with download
    }
    
    await downloadBinary(owner, repo, packageInfo.version, binaryName, platformInfo, targetPath);
    
  } catch (error) {
    console.error('Failed to install binary:', error);
    process.exit(1);
  }
}

if (require.main === module) {
  install();
}

export { install };
