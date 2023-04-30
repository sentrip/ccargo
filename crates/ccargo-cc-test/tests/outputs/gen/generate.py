import os
import shutil
import subprocess
import platform
from pathlib import Path

WIN = platform.system() == 'Windows'
SRC_DIR = Path(__file__).parent
OUT_DIR = SRC_DIR / 'build'
EXT_OBJ = '.obj' if WIN else '.o'
EXT_BIN = '.exe' if WIN else ''
EXE = f'build/main{EXT_BIN}'

def tool_exists(name):
    exe = f'{name}{EXT_BIN}'
    if WIN:
        for p in os.environ['PATH'].split(';'):
            path = Path(p) / exe
            if path.exists():
                return True
        return False
    else:
        return Path(f'/usr/bin/{name}').exists()


MSVC = tool_exists('cl')
GCC = tool_exists('gcc')
CLANG = tool_exists('clang')


def run_cmd(args):
    return subprocess.run(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE, cwd=SRC_DIR)


def gcc(srcs, colors=False, force_link_warning=False):
    extra = ['-Wl,-entry=main1'] if force_link_warning else []
    extra += ['-fdiagnostics-color=always'] if colors else []
    return run_cmd(['gcc', '-o', EXE] + srcs + extra)


def clang(srcs, colors=False, force_link_warning=False):
    extra = ['-Wl,-entry=main1'] if force_link_warning else []
    extra += ['-fcolor-diagnostics', '-fansi-escape-codes'] if colors else []
    return run_cmd(['clang', '-o', str(EXE)] + srcs + extra)


def msvc(srcs, warnings_errors=False, linker_warnings_errors=False, **kwargs):
    if len(srcs) == 1:
        obj_out = 'build/' + srcs[0].replace('.c', EXT_OBJ)
    else:
        obj_out = 'build/'
    args = ['cl.exe', '-nologo', '-diagnostics:column']
    if warnings_errors:
        args.append('-WX')
    args.extend([f'-Fo{obj_out}', f'-Fe{EXE}'])
    args.extend(srcs)
    if linker_warnings_errors:
        args.extend(['/link', '/WX'])
    return run_cmd(args)


SUCCESS = [
    ['main.c'],
    ["""int main(void) { return 0; }"""]
]

COMPILE_WARNING = [
    ['main.c'],
    ["""
int main(void) { 
    long long v = 0;
    int* x = &v;
    int* y = &v;
    return 0; 
}
"""]]

COMPILE_ERROR = [
    ['main.c'],
    ["""
void func() {}
int main(void) { 
    int x = func();
    int y = func();
    return 0; 
}
"""]]

LINK_WARNING = [
    ['warn.c', 'main.c'],
    ["""
#if defined(_MSC_VER)
__declspec(dllexport) 
#else
__attribute__((visibility("default")))
#endif
void func(void) {}
""", 
"""
#if defined(_MSC_VER)
__declspec(dllimport) 
#else
__attribute__((visibility("hidden")))
#endif
void func(void);
int main(void) { func(); return 0; }
"""],
]

LINK_ERROR = [
    ['warn.c', 'main.c'],
    ["""
void func(void) {}
""",
"""
void func(void) {}
int main(void) { func(); return 0; }
"""],
]

COMPILE_WARNING_ERROR = COMPILE_WARNING
LINK_WARNING_ERROR = LINK_WARNING


def clean():
    shutil.rmtree(OUT_DIR, ignore_errors=True)
    for path in SRC_DIR.iterdir():
        if path.suffix == '.c':
            os.remove(path)


def setup():
    clean()
    os.mkdir(OUT_DIR)


def sources(srcs):
    for (path, src) in zip(srcs[0], srcs[1]):
        with open(SRC_DIR / path, 'w') as f:
            f.write(src)


def collect(
    func_name,
    srcs_name,
    colors=False,
    **kwargs
):
    out_dir = SRC_DIR.parent \
        / platform.system().lower() \
        / func_name \
        / ('colors' if colors else 'plain')
    out_dir.mkdir(parents=True, exist_ok=True)
    r = globals()[func_name](globals()[srcs_name][0], colors=colors, **kwargs)
    if r.stdout:
        with open(out_dir / f'{srcs_name.lower()}.stdout.in', 'wb') as f:
            f.write(r.stdout)
    if r.stderr:
        with open(out_dir / f'{srcs_name.lower()}.stderr.in', 'wb') as f:
            f.write(r.stderr)


def collect_all(
    srcs_name,
    colors=False,
    **kwargs_gcc_clang
):
    setup()
    sources(globals()[srcs_name])
    if GCC:
        collect('gcc', srcs_name, colors=colors, **kwargs_gcc_clang)
    if CLANG:
        collect('clang', srcs_name, colors=colors, **kwargs_gcc_clang)
    if MSVC and not colors:
        collect('msvc', srcs_name, colors=colors)


if __name__ == '__main__':
    for colors in [False, True]:
        collect_all('COMPILE_WARNING', colors=colors)
        collect_all('COMPILE_ERROR', colors=colors)
        collect_all('LINK_WARNING', colors=colors, force_link_warning=not WIN)
        collect_all('LINK_ERROR', colors=colors)
    if MSVC:
        sources(COMPILE_WARNING)
        collect('msvc', 'COMPILE_WARNING_ERROR', warnings_errors=True)        
        sources(LINK_WARNING)
        collect('msvc', 'LINK_WARNING_ERROR', linker_warnings_errors=True)
    clean()
