"""Check Native Chart options gating and legacy wire fields without a running GUI."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile

def run(exe, capabilities, directory):
    params={'launcher':'99.0.0'}
    if capabilities is not None:params['capabilities']=capabilities
    requests=[{'id':1,'method':'hello','params':params},{'id':2,'method':'view'},
              {'id':3,'method':'event','params':{'event':{'click':{'id':'change-style'}}}},
              {'id':4,'method':'view'},{'id':5,'method':'tick'}]
    result=subprocess.run([str(exe)],input=''.join(json.dumps(r)+'\n' for r in requests),text=True,capture_output=True,cwd=directory,timeout=10,check=True)
    replies=[json.loads(line) for line in result.stdout.splitlines()]
    assert [r['id'] for r in replies]==[1,2,3,4,5]
    assert replies[0]['result']=={'api':1}
    assert replies[2]['result']=='reload'
    assert replies[4]['result'] is None
    return replies[1]['result']['body']['blocks'],replies[3]['result']['body']['blocks']

def main():
    exe=Path(sys.argv[1]).resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix='native-chart-options-') as directory:
        for caps in [None,[],['view.chart.options'],['future.other']]:
            before,_=run(exe,caps,directory);assert not any('chart' in b for b in before)
        for caps in [['view.chart'],['view.chart','future.other']]:
            before,_=run(exe,caps,directory)
            assert len(before)==3
            for block in before:
                c=block['chart'];assert 'options' not in c and 'images' not in c
                for s in c['series']:
                    assert 'line_width' not in s and 'blocks' not in s
                    for cell in s.get('cells',[]):assert 'style' not in cell
        before,after=run(exe,['view.chart','view.chart.options','future.other'],directory)
        assert before[0]['chart']['images']
        assert before[0]['chart']['options']['text']['title']==19
        assert after[0]['chart']['options']['text']['title']==28
        assert before[1]['chart']['series'][1]['line_width']==4
        assert not list(Path(directory).iterdir()),'Example wrote files'
    print('PASS: seven options, capabilities, older/future hosts, reload, independent text sizes, no generated files')

if __name__=='__main__':main()
