'use strict';
const reportCapability = new URLSearchParams(location.hash.slice(1)).get('fixture') || '';
history.replaceState(null, '', location.pathname + location.search);
const widths = [320, 390, 768, 1024, 1440], surfaces = ['gallery', 'files', 'settings', 'jobs', 'preview'], themes = ['light', 'dark'];
const cases = themes.flatMap(theme => widths.flatMap(width => surfaces.map(surface => ({ width, surface, theme })))), results = [];
const frame = document.getElementById('frame'), choice = document.getElementById('case');
for (const [index, item] of cases.entries()) { const option = document.createElement('option'); option.value = index; option.textContent = `${item.width}px · ${item.surface} · ${item.theme}`; choice.append(option); }
let running = false, cursor = 0, awaiting = false;
function show(item) { awaiting = true; frame.width = item.width; document.getElementById('case-title').textContent = `${item.width}px · ${item.surface} · ${item.theme}`; frame.src = `/case?surface=${item.surface}&caseWidth=${item.width}&theme=${item.theme}&run=${Date.now()}`; }
function render() {
  document.getElementById('results').replaceChildren(...results.map(report => {
    const row = document.createElement('tr');
    const values = [report.width, `${report.surface} · ${report.theme}`, report.passed ? 'PASS' : 'FAIL', `${report.controls || 0} at ${report.minimumControlPx || '?'}px`, (report.failures || []).join(' · ') || (report.interactionChecks || []).join(' · ') || 'No overflow; control sizes passed'];
    values.forEach((value, index) => { const cell = document.createElement('td'); cell.textContent = value; if (index === 2) cell.className = report.passed ? 'pass' : 'fail'; row.append(cell); }); return row;
  }));
  document.getElementById('json').textContent = JSON.stringify(results, null, 2);
}
window.addEventListener('message', async event => {
  if (event.origin !== location.origin || event.source !== frame.contentWindow || event.data?.type !== 'photofindry-layout-report' || !awaiting) return;
  awaiting = false;
  if (!running) { document.getElementById('summary').textContent = `${event.data.report.width}px ${event.data.report.surface} ${event.data.report.theme}: ${event.data.report.passed ? 'PASS' : 'FAIL'}`; return; }
  results.push(event.data.report); render(); cursor += 1;
  document.getElementById('summary').textContent = `Checked ${results.length} of ${cases.length} cases · ${results.filter(result => !result.passed).length} failed`;
  if (cursor < cases.length) show(cases[cursor]);
  else { running = false; document.getElementById('run').disabled = false; document.getElementById('summary').textContent = `Complete: ${results.filter(result => result.passed).length}/${cases.length} passed · ${results.filter(result => !result.passed).length} failed`;
    await fetch('/report', { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-PhotoFindry-Fixture': reportCapability }, body: JSON.stringify({ cases: results, completedAt: new Date().toISOString(), userAgent: navigator.userAgent }) }); }
});
document.getElementById('run').addEventListener('click', () => { if (running) return; running = true; cursor = 0; results.length = 0; render(); document.getElementById('run').disabled = true; show(cases[0]); });
document.getElementById('inspect').addEventListener('click', () => { if (!running) show(cases[Number(choice.value)]); });
const parameters = new URL(location.href).searchParams;
if (parameters.get('visual') === '1') {
  const selected = cases.findIndex(item => item.width === Number(parameters.get('width')) && item.surface === parameters.get('surface') && item.theme === (parameters.get('theme') || 'light'));
  if (selected >= 0) { choice.value = selected; document.body.classList.add('visual-only'); show(cases[selected]); }
  else document.getElementById('summary').textContent = 'Choose a supported width and workflow.';
} else document.getElementById('run').click();
