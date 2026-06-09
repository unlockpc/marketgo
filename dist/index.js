#!/usr/bin/env node
/**
 * UnMarket - Global Marketing Automation Tool
 *
 * Input a product URL, auto-generate multilingual content
 * and publish to 200+ platforms worldwide.
 *
 * Part of UnzooAI product family:
 * - Unzoo: AI's hands (browser automation)
 * - SoloMD: AI's eyes (document processing)
 * - UnMarket: AI's voice (global marketing)
 */
import { Command } from 'commander';
import chalk from 'chalk';
import { configCommand } from './cli/config.js';
import { accountCommand } from './cli/account.js';
import { productCommand } from './cli/product.js';
import { publishCommand, previewCommand } from './cli/publish.js';
import { browserCommand } from './cli/browser.js';
import { aiCommand } from './cli/ai.js';
import { runCommand } from './cli/run.js';
import { statsCommand } from './cli/stats.js';
import { analyzeCommand } from './cli/analyze.js';
import { registerCommand } from './cli/register.js';
import { gmailCommand } from './cli/gmail.js';
import { queueCommand } from './cli/queue.js';
import { initializeApp } from './core/init.js';
const VERSION = '0.1.0';
const program = new Command();
program
    .name('unmarket')
    .description(chalk.cyan('UnMarket') + ' - Global marketing automation tool\n\n' +
    '  Input a product URL, auto-generate multilingual content\n' +
    '  and publish to 200+ platforms worldwide.')
    .version(VERSION, '-v, --version', 'Display version number')
    .option('--debug', 'Enable debug mode')
    .hook('preAction', async () => {
    await initializeApp();
});
// Main command: unmarket <url>
program
    .argument('[url]', 'Product URL to analyze and publish')
    .option('--preview', 'Preview generated content without publishing')
    .option('--platforms <platforms>', 'Comma-separated list of platforms')
    .option('--languages <languages>', 'Comma-separated list of languages')
    .option('--dry-run', 'Simulate publishing without actually posting')
    .action(async (url, options) => {
    if (url) {
        const { handleMainCommand } = await import('./cli/main.js');
        await handleMainCommand(url, options);
    }
    else {
        program.help();
    }
});
// Config commands
program.addCommand(configCommand);
// Gmail commands
program.addCommand(gmailCommand);
// Account commands
program.addCommand(accountCommand);
// Register commands
program.addCommand(registerCommand);
// Product commands
program.addCommand(productCommand);
// Analyze command
program.addCommand(analyzeCommand);
// Publish command
program.addCommand(publishCommand);
// Preview command
program.addCommand(previewCommand);
// Browser commands
program.addCommand(browserCommand);
// AI commands
program.addCommand(aiCommand);
// Run scheduler
program.addCommand(runCommand);
// Queue management
program.addCommand(queueCommand);
// Stats commands
program.addCommand(statsCommand);
// Parse and execute
program.parse();
//# sourceMappingURL=index.js.map