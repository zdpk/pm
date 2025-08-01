#!/usr/bin/env node

import { spawn } from 'child_process';
import { join } from 'path';

const binaryName = 'pm';

async function main() {
  const binaryPath = join(__dirname, '..', 'bin', binaryName + (process.platform === 'win32' ? '.exe' : ''));
  
  const child = spawn(binaryPath, process.argv.slice(2), {
    stdio: 'inherit'
  });
  
  child.on('error', (error) => {
    if (error.message.includes('ENOENT')) {
      console.error(`Binary not found: ${binaryPath}`);
      console.error('Please ensure the binary was installed correctly.');
      process.exit(1);
    } else {
      console.error('Error running binary:', error.message);
      process.exit(1);
    }
  });
  
  child.on('close', (code) => {
    process.exit(code || 0);
  });
}

if (require.main === module) {
  main().catch((error) => {
    console.error('Unexpected error:', error);
    process.exit(1);
  });
}
