const fs = require('fs');
const path = require('path');

const claudeDir = process.env.CLAUDE_CONFIG_DIR || path.join(require('os').homedir(), '.claude');
const settingsPath = path.join(claudeDir, 'settings.json');

const command = `bash -c 'cols=\${COLUMNS:-}; case "\$cols" in ""|*[!0-9]*) cols=\$(stty size </dev/tty 2>/dev/null | awk '"'"'{print \$2}'"'"');; esac; case "\$cols" in ""|*[!0-9]*) cols=120;; esac; export COLUMNS=\$(( cols > 4 ? cols - 4 : 1 )); plugin_dir=\$(ls -d "\${CLAUDE_CONFIG_DIR:-\$HOME/.claude}"/plugins/cache/*/claude-hud/*/ 2>/dev/null | awk -F/ '"'"'{ print \$(NF-1) "\\t" \$(0) }'"'"' | grep -E '"'"'^[0-9]+\\.[0-9]+\\.[0-9]+[[:space:]]'"'"' | sort -t. -k1,1n -k2,2n -k3,3n -k4,4n | tail -1 | cut -f2-); exec node "\${plugin_dir}dist/index.js"'`;

const settings = JSON.parse(fs.readFileSync(settingsPath, 'utf8'));
settings.statusLine = { type: 'command', command };
fs.writeFileSync(settingsPath, JSON.stringify(settings, null, 2));
console.log('Config written to', settingsPath);
