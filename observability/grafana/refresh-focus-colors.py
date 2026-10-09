#!/usr/bin/env python3
"""Apply shared project/workspace and app colors to Grafana dashboards."""
import argparse
import colorsys
import hashlib
import json
from pathlib import Path
import urllib.parse
import urllib.request

PALETTE = (
    '#5794F2', '#FF9830', '#B877D9', '#F2495C', '#FADE2A', '#73BF69',
    '#FF73BF', '#8AB8FF', '#FFB357', '#DEB6F2', '#F29191', '#B8E986',
    '#56D9D3', '#B3A2F7', '#FFCC99', '#C2D1FF',
)


def assign_colors(names, previous):
    # Preserve prior assignments as new names appear. Both panels share a
    # vocabulary so a project/workspace with the same name has the same color.
    assigned = {name: previous[name] for name in sorted(names) if name in previous}
    used = set(assigned.values())
    for name in sorted(names):
        if name in assigned:
            continue
        seed = int.from_bytes(hashlib.sha256(name.encode()).digest()[:4], 'big')
        start = seed % len(PALETTE)
        color = next((PALETTE[(start+i) % len(PALETTE)] for i in range(len(PALETTE))
                      if PALETTE[(start+i) % len(PALETTE)] not in used), None)
        # More categories than the base palette: generate an unused bright hue.
        hue = seed / 2**32
        while color is None or color in used:
            rgb = colorsys.hls_to_rgb(hue % 1, 0.65, 0.7)
            color = '#' + ''.join(f'{round(c*255):02X}' for c in rgb)
            hue += 0.61803398875
        assigned[name] = color
        used.add(color)
    return assigned


def metric_names(url, dimension):
    return find_names(url, f'rsynapse.focus.{dimension}.*')


def find_names(url, pattern):
    query = urllib.parse.urlencode({'query': pattern, 'format': 'treejson'})
    with urllib.request.urlopen(url.rstrip('/') + '/metrics/find?' + query, timeout=10) as response:
        return {item['text'] for item in json.load(response)}


def series_overrides(names, colors):
    return [
        {'matcher': {'id': 'byName', 'options': name},
         'properties': [{'id': 'color', 'value': {'mode': 'fixed', 'fixedColor': colors[name]}}]}
        for name in sorted(names)
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--graphite-url', default='http://127.0.0.1:8080')
    parser.add_argument('--dashboard', type=Path, default=Path(__file__).parent / 'dashboards/rsynapse-focus.json')
    parser.add_argument('--workspace-dashboard', type=Path, default=Path(__file__).parent / 'dashboards/rsynapse-workspace.json')
    parser.add_argument('--registry', type=Path, default=Path(__file__).parent / 'focus-colors.json')
    parser.add_argument('--check', action='store_true', help='Validate without writing; exit nonzero if refresh is needed')
    args = parser.parse_args()
    dashboard = json.loads(args.dashboard.read_text())
    workspace_dashboard = json.loads(args.workspace_dashboard.read_text())
    panels = {p['id']: p for p in dashboard['panels'] if p['id'] in (1, 7)}
    names = {1: metric_names(args.graphite_url, 'project'), 7: metric_names(args.graphite_url, 'workspace_name')}
    agent_projects = find_names(args.graphite_url, 'rsynapse.agents.project.*')
    previous = {}
    for panel in panels.values():
        for mapping in panel['fieldConfig']['defaults'].get('mappings', []):
            if mapping['type'] == 'value':
                previous.update({name: entry['color'] for name, entry in mapping['options'].items()
                                 if name != '(none)' and 'color' in entry})
    registry = json.loads(args.registry.read_text()) if args.registry.exists() else {'contexts': {}, 'apps': {}}
    previous.update(registry['contexts'])
    colors = assign_colors(names[1] | names[7] | agent_projects | set(previous), previous)
    app_names = metric_names(args.graphite_url, 'app') | find_names(args.graphite_url, 'rsynapse.focus.workspace_name.*.app.*')
    app_colors = assign_colors(app_names | set(registry['apps']), registry['apps'])
    updated_registry = {'contexts': colors, 'apps': app_colors}
    before_overview = json.dumps(dashboard)
    before_workspace = json.dumps(workspace_dashboard)
    for panel_id, panel in panels.items():
        options = {'(none)': {'text': 'None', 'color': 'gray'}}
        options.update({name: {'text': name, 'color': colors[name]} for name in sorted(names[panel_id])})
        mapping = [{'type': 'value', 'options': options}]
        defaults = panel['fieldConfig']['defaults']
        defaults['mappings'] = mapping
    all_panels = {p['id']: p for p in dashboard['panels']}
    all_panels[8]['fieldConfig']['overrides'] = series_overrides(names[1], colors)
    all_panels[16]['fieldConfig']['overrides'] = series_overrides(names[7], colors)
    all_panels[2]['fieldConfig']['overrides'] = series_overrides(app_names, app_colors)
    for panel_id in (18, 19, 20):
        if panel_id in all_panels:
            all_panels[panel_id]['fieldConfig']['overrides'] = series_overrides(agent_projects, colors)
    detail_panels = {p['id']: p for p in workspace_dashboard['panels']}
    detail_panels[12]['fieldConfig']['overrides'] = series_overrides(app_names, app_colors)
    detail_panels[17]['fieldConfig']['overrides'] = series_overrides(names[7], colors)
    # Numeric focus states inherit the selected workspace field's color;
    # inactive remains gray. Text still distinguishes active vs idle.
    detail_panels[17]['fieldConfig']['defaults']['mappings'] = [{
        'type': 'value', 'options': {
            '0': {'text': 'Not focused', 'color': 'gray'},
            '1': {'text': 'Active focus'}, '2': {'text': 'Idle focus'},
        }
    }]
    changed = (before_overview != json.dumps(dashboard) or before_workspace != json.dumps(workspace_dashboard)
               or registry != updated_registry or not args.registry.exists())
    if args.check:
        if changed:
            raise SystemExit('New categories need colors; run this tool without --check')
    elif changed:
        args.dashboard.write_text(json.dumps(dashboard, indent=2) + '\n')
        args.workspace_dashboard.write_text(json.dumps(workspace_dashboard, indent=2) + '\n')
        args.registry.write_text(json.dumps(updated_registry, indent=2) + '\n')
    print(f'{len(colors)} context colors and {len(app_colors)} app colors shared across panels; None remains gray')


if __name__ == '__main__':
    main()
