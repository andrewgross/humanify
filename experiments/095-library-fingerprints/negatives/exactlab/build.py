"""Exact-version mini-lab: npm install (--ignore-scripts; nothing executed), entries dyn/req/imp, Bun 1.4.0 --minify builds.
usage: python3 build.py [--jobs 10]"""
import os, subprocess, sys, json, re, concurrent.futures as cf
E = os.path.dirname(os.path.abspath(__file__))
TARGET = os.environ.get('TARGET', 'node')
BDIR = 'bun-1.4.0' + ('-tbun' if TARGET == 'bun' else '')
BUN = '/work/exp095-library-fingerprints/lab/tools/bun-1.4.0/node_modules/.bin/bun'
def parse():
    for l in open(f'{E}/specs.txt'):
        if not l.strip() or l.startswith('#'): continue
        app, specs, imps, fmt = [x.strip() for x in l.split('|')]
        yield app, specs.split(), imps.split(','), fmt
def one(app, specs, imps, fmt):
    d = f'{E}/pkgs/{app}'; os.makedirs(d + '/entries', exist_ok=True)
    if not os.path.exists(d + '/.installed'):
        open(d + '/package.json', 'w').write('{"private":true}')
        r = subprocess.run(['npm', 'install', '--no-audit', '--no-fund', '--ignore-scripts', *specs], cwd=d, capture_output=True, text=True)
        open(f'{E}/logs/install-{app}.log', 'w').write(r.stdout + r.stderr)
        if r.returncode: return app, 'install failed'
        subprocess.run(f"grep -rlZ --include='*.js' --include='*.mjs' --include='*.cjs' -E '^//[#@] sourceMappingURL=' node_modules | xargs -0 -r sed -i -E '/^\\/\\/[#@] sourceMappingURL=/d'", cwd=d, shell=True)
        open(d + '/.installed', 'w').write('ok')
    tag = app.replace('-', '_').replace('.', '_')
    j = lambda xs: ','.join(json.dumps(x) for x in xs)
    open(d + '/entries/dyn.mjs', 'w').write(f'function labAppMain_dyn(x){{ if (!x) throw new Error("lab-app dyn {tag}: no input"); return Object.keys(x).length + ":dyn"; }}\nexport const loaders = [{",".join(f"() => import({json.dumps(i)})" for i in imps)}];\nexport {{ labAppMain_dyn }};\n')
    open(d + '/entries/imp.mjs', 'w').write(f'function labAppMain_imp(x){{ if (!x) throw new Error("lab-app imp {tag}: no input"); return Object.keys(x).length + ":imp"; }}\n' + ''.join(f'import * as m{k} from {json.dumps(i)};\n' for k, i in enumerate(imps)) + f'export {{ labAppMain_imp, {", ".join(f"m{k}" for k in range(len(imps)))} }};\n')
    open(d + '/entries/req.cjs', 'w').write(f'function labAppMain_req(x){{ if (!x) throw new Error("lab-app req {tag}: no input"); return Object.keys(x).length + ":req"; }}\n' + ''.join(f'const m{k} = require({json.dumps(i)});\n' for k, i in enumerate(imps)) + f'module.exports = {{ run: labAppMain_req, {", ".join(f"m{k}" for k in range(len(imps)))} }};\n')
    res = []
    for mode in ('dyn', 'imp', 'req'):
        if fmt == 'esm' and mode == 'req': continue
        entry = f'{d}/entries/{mode}.' + ('cjs' if mode == 'req' else 'mjs')
        out = f'{E}/builds/{BDIR}/min/{app}/{mode}'
        if os.path.exists(out + '/out.js.map'): res.append(mode); continue
        os.makedirs(out + '/.tmp', exist_ok=True)
        r = subprocess.run([BUN, 'build', entry, '--target', TARGET, '--format', fmt, '--minify', '--sourcemap=external', '--outdir', out + '/.tmp'] + (['--external', 'react-devtools-core'] if fmt == 'esm' else []), cwd=d + '/entries', capture_output=True, text=True)
        base = os.path.basename(entry).rsplit('.', 1)[0]
        if r.returncode or not os.path.exists(f'{out}/.tmp/{base}.js'):
            open(f'{E}/logs/build-{app}-{mode}.log', 'w').write(r.stdout + r.stderr); continue
        os.rename(f'{out}/.tmp/{base}.js', out + '/out.js'); os.rename(f'{out}/.tmp/{base}.js.map', out + '/out.js.map')
        subprocess.run(['rm', '-rf', out + '/.tmp']); res.append(mode)
    return app, res
jobs = int(sys.argv[sys.argv.index('--jobs') + 1]) if '--jobs' in sys.argv else 10
with cf.ThreadPoolExecutor(jobs) as ex:
    for app, r in ex.map(lambda a: one(*a), list(parse())): print(app, r, flush=True)
