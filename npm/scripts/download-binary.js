const https = require('https');
const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

const REPO_OWNER = 'zdpk'; 
const REPO_NAME = 'pm'; 

async function downloadBinary() {
    const packageJsonPath = path.join(__dirname, '..', 'package.json');
    const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
    const version = packageJson.version; // package.json에서 버전 읽기

    const {arch,platform} = process;

    let assetName;
    let binaryName = 'pm';

    if (platform === 'linux' && arch === 'x64') {
        assetName = 'pm-linux-x64';
    } else if (platform === 'darwin' && arch === 'x64') {
        assetName = 'pm-macos-x64';
    } else if (platform === 'darwin' && arch === 'arm64') {
        assetName = 'pm-macos-arm64';
    } else if (platform === 'win32' && arch === 'x64') {
        assetName = 'pm-windows-x64.exe';
        binaryName = 'pm.exe';
    } else {
        console.error(`Unsupported platform: ${platform}-${arch}`);
        process.exit(1);
    }

    try {
        const releaseInfo = await getReleaseByTag(version); // 특정 버전 릴리스 정보 가져오기
        const asset = releaseInfo.assets.find(a => a.name === assetName);

        if (!asset) {
            console.error(`Binary asset '${assetName}' not found for release v${version}.`);
            process.exit(1);
        }

        const downloadUrl = asset.browser_download_url;
        const binDir = path.join(__dirname, '..', 'bin');
        const outputPath = path.join(binDir, binaryName);

        if (!fs.existsSync(binDir)) {
            fs.mkdirSync(binDir, { recursive: true });
        }

        console.log(`Downloading ${assetName} from ${downloadUrl} to ${outputPath}...`);
        await downloadFile(downloadUrl, outputPath);
        console.log('Download complete.');

        if (platform !== 'win32') {
            execSync(`chmod +x ${outputPath}`);
            console.log(`Set executable permissions for ${outputPath}`);
        }

    } catch (error) {
        console.error('Error downloading binary:', error.message);
        process.exit(1);
    }
}

function getReleaseByTag(tag) {
    return new Promise((resolve, reject) => {
        const options = {
            hostname: 'api.github.com',
            path: `/repos/${REPO_OWNER}/${REPO_NAME}/releases/tags/v${tag}`, // v 접두사 추가
            headers: {
                'User-Agent': 'Node.js'
            }
        };

        https.get(options, (res) => {
            let data = '';
            res.on('data', (chunk) => data += chunk);
            res.on('end', () => {
                if (res.statusCode === 200) {
                    resolve(JSON.parse(data));
                } else {
                    reject(new Error(`Failed to get release v${tag}: ${res.statusCode} - ${data}`));
                }
            });
        }).on('error', reject);
    });
}

function downloadFile(url, outputPath) {
    return new Promise((resolve, reject) => {
        https.get(url, (response) => {
            if (response.statusCode === 200) {
                const fileStream = fs.createWriteStream(outputPath);
                response.pipe(fileStream);
                fileStream.on('finish', () => {
                    fileStream.close();
                    resolve();
                });
            } else {
                reject(new Error(`Failed to download file: ${response.statusCode}`));
            }
        }).on('error', reject);
    });
}

downloadBinary();
