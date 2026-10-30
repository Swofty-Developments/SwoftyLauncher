import json
import os
import struct
import sys

TEAM = sys.argv[4]
TEX = {'paint': f'{TEAM}_paint_d.png', 'detail': f'{TEAM}_details_d.png', 'wheel': f'{TEAM}_wheel_d.png',
       'steering_wheel': f'{TEAM}_steering_wheel_d.png'}


def main(glb, tex_dir, out):
    b = open(glb, 'rb').read()
    jl = struct.unpack_from('<I', b, 12)[0]
    j = json.loads(b[20:20 + jl])
    binc = b[28 + jl:]
    j.setdefault('images', [])
    j.setdefault('textures', [])
    j['samplers'] = [{'magFilter': 9729, 'minFilter': 9987, 'wrapS': 10497, 'wrapT': 10497}]
    rel = os.path.relpath(tex_dir, os.path.dirname(out))
    for m in j['materials']:
        f = TEX.get(m['name'])
        if f:
            j['images'].append({'uri': f'{rel}/{f}'})
            j['textures'].append({'sampler': 0, 'source': len(j['images']) - 1})
            m['pbrMetallicRoughness']['baseColorTexture'] = {'index': len(j['textures']) - 1}
            m['pbrMetallicRoughness']['baseColorFactor'] = [1, 1, 1, 1]
    js = json.dumps(j, separators=(',', ':')).encode()
    js += b' ' * ((4 - len(js) % 4) % 4)
    with open(out, 'wb') as f:
        f.write(struct.pack('<III', 0x46546C67, 2, 12 + 8 + len(js) + 8 + len(binc)))
        f.write(struct.pack('<II', len(js), 0x4E4F534A))
        f.write(js)
        f.write(struct.pack('<II', len(binc), 0x004E4942))
        f.write(binc)


if __name__ == '__main__':
    main(*sys.argv[1:4])
