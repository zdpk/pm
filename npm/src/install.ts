import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import fetch from 'node-fetch';
import decompress, { File as DecompressFile } from 'decompress';

const REPO_OWNER = 'zdpk';
const REPO_NAME = 'pm';
const BIN_DIR = path.join(__dirname, '..', 'bin');

async function getLatestReleaseTag(): Promise<string> {
    const packageJsonPath = path.join(__dirname, '..', 'package.json');
    const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
    return `v${packageJson.version}`;
}

function getDownloadUrl(version: string): { url: string, filename: string } {
    const platform = os.platform();
    const arch = os.arch();

    let target = '';
    let ext = '.tar.gz'; // Default for Linux/macOS

    if (platform === 'linux' && arch === 'x64') {
        target = 'x86_64-unknown-linux-gnu';
    } else if (platform === 'darwin' && arch === 'x64') {
        target = 'x86_64-apple-darwin';
    } else if (platform === 'darwin' && arch === 'arm64') {
        target = 'aarch64-apple-darwin';
    } else if (platform === 'win32' && arch === 'x64') {
        target = 'x86_64-pc-windows-msvc';
        ext = '.zip'; // Windows binaries are typically .zip
    } else {
        throw new Error(`Unsupported platform: ${platform}-${arch}`);
    }

    const filename = `pm-${target}${ext}`;
    const url = `https://github.com/${REPO_OWNER}/${REPO_NAME}/releases/download/${version}/${filename}`;
    return { url, filename };
}

async function downloadAndExtractBinary() {
    try {
        const version = await getLatestReleaseTag();
        const { url: downloadUrl, filename: expectedFilename } = getDownloadUrl(version);
        const response = await fetch(downloadUrl);

        if (!response.ok) {
            throw new Error(`Failed to download binary from ${downloadUrl}: ${response.statusText}`);
        }

        const arrayBuffer = await response.arrayBuffer();
        const buffer = Buffer.from(arrayBuffer);

        if (!fs.existsSync(BIN_DIR)) {
            fs.mkdirSync(BIN_DIR, { recursive: true });
        }

        await decompress(buffer, BIN_DIR, {
            filter: (file: DecompressFile) => file.path.includes('pm') || file.path.includes('pm.exe'),
            strip: 1 // Remove the top-level directory inside the archive
        });

        const binaryPath = path.join(BIN_DIR, os.platform() === 'win32' ? 'pm.exe' : 'pm');
        fs.chmodSync(binaryPath, '755'); // Grant execute permissions

        console.log(`pm binary downloaded to ${binaryPath}`);
    } catch (error) {
        console.error('Error downloading or extracting pm binary:', error);
        process.exit(1);
    }
}

downloadAndExtractBinary();
