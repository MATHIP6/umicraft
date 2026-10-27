import re,os,json
root=os.environ["MCP_BLOCK_DIR"]  # .../mcp/src/minecraft/net/minecraft/block/
src=open(root+'Block.java').read()
start=src.index('public static void registerBlocks()')
body=src[start:]
# variable defs
vars_={}
for m in re.finditer(r'Block (\w+) = (.+?);\n',body):
    vars_[m.group(1)]=m.group(2)
cls_cache={}
def cls(name):
    if name not in cls_cache:
        p=root+name+'.java'
        cls_cache[name]=open(p).read() if os.path.exists(p) else ''
    return cls_cache[name]
def material(expr,name):
    m=re.search(r'Material\.(\w+)',expr)
    if m: return m.group(1)
    if name and name!='Block':
        t=cls(name)
        m=re.search(r'super\((?:\s*)Material\.(\w+)',t)
        if m: return m.group(1)
        m=re.search(r'extends (\w+)',t)
        if m and m.group(1)!='Block': return material('',m.group(1))
        m=re.search(r'Material\.(\w+)',t)
        if m: return m.group(1)
    return 'unknown'
def hardness(expr,name):
    if 'setBlockUnbreakable' in expr: return -1.0
    m=re.search(r'setHardness\(([-\d.]+)F\)',expr)
    if m: return float(m.group(1))
    while name and name!='Block':
        t=cls(name)
        if 'setBlockUnbreakable' in t: return -1.0
        m=re.search(r'setHardness\(([-\d.]+)F\)',t)
        if m: return float(m.group(1))
        m=re.search(r'extends (\w+)',t)
        name=m.group(1) if m else None
    return 0.0
out={}
for m in re.finditer(r'registerBlock\((\d+), "(\w+)", (.+)\);\n',body):
    i,name,expr=int(m.group(1)),m.group(2),m.group(3)
    if re.fullmatch(r'\w+',expr.strip()): expr=vars_.get(expr.strip(),expr)
    cm=re.search(r'new (Block\w*)\(',expr)
    c=cm.group(1) if cm else 'Block'
    out[i]=(name,hardness(expr,c),material(expr,c),c)
# registerBlock with block+itemblock variants
json.dump(out,open('/dev/null','w'))
print(len(out))
print([ (i,v) for i,v in out.items() if v[2]=='unknown'][:20])
print(sorted(set(v[2] for v in out.values())))

stairs={53:5,134:5,135:5,136:5,163:5,164:5,67:4,108:45,109:98,114:112,128:24,156:155,180:179}
for s,b in stairs.items():
    n=out[s][0]
    out[s]=(n,out[b][1],out[b][2],'BlockStairs')
out[139]=(out[139][0],2.0,'rock','BlockWall')
for i in (176,177):
    out[i]=(out[i][0],out[i][1],'wood',out[i][3])
mats=sorted(set(v[2] for v in out.values())|{'air','coral'})
mats=[m for m in mats if m not in('unknown','isOpaque')]
def pas(m): return m[0].upper()+m[1:]
lines=[]
lines.append('// SPDX-License-Identifier: MIT OR Apache-2.0\n')
lines.append('//! Block hardness and material of the 1.8 blocks, by block id.\n//!\n//! Generated from the `Block.registerBlocks` table of the reference (MCP\n//! `net/minecraft/block/Block.java`, `setHardness` / `setBlockUnbreakable`\n//! and the constructor material of each block class), see `docs/sources.md`.\n')
lines.append('/// A block material (MCP `net/minecraft/block/material/Material`), reduced to\n/// the ones that decide how a block is dug.\n/// Some materials name no block of the table (air has its own id check), but\n/// the tool rules match on them.\n#[allow(dead_code)]\n#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]\npub enum Material {')
for m in mats: lines.append(f'    /// MCP `Material.{m}`.\n    {pas(m)},')
lines.append('    /// A block without a table entry.\n    #[default]\n    Other,\n}\n')
lines.append('/// The digging data of one block.\n#[derive(Debug, Clone, Copy, PartialEq, Default)]\npub struct BlockData {\n    /// MCP `Block.blockHardness`; negative means unbreakable.\n    pub hardness: f32,\n    /// The block material.\n    pub material: Material,\n}\n')
lines.append('const NONE: BlockData = BlockData {\n    hardness: 0.0,\n    material: Material::Other,\n};\n')
lines.append('/// The data of every block id below 256; unknown ids are [`Material::Other`]\n/// with hardness 0.\npub(crate) static BLOCKS: [BlockData; 256] = {\n    let mut table = [NONE; 256];')
for i in sorted(out):
    n,h,m,c=out[i]
    if m in('unknown','isOpaque'): m='other'
    lines.append(f'    table[{i}] = BlockData {{ hardness: {h!r}, material: Material::{pas(m)} }}; // {n}')
lines.append('    table\n};')
open(os.environ.get('OUT','block_data.rs'),'w').write('\n'.join(lines)+'\n')
names={v[0]:i for i,v in out.items()}
json.dump(names,open('/dev/null','w'))
print(mats)
