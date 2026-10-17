"""Full unmodified/patched ths executable, real disposable Regtest, closed stdout pipe.
No HTTP/Docker/CLI shims; only the caller's result pipe is deliberately closed.
"""
import argparse, hashlib, json, os, shutil, signal, socket, subprocess, time, uuid
from pathlib import Path
from urllib.request import Request, urlopen
os.umask(0o077)
a=argparse.ArgumentParser();a.add_argument('--binary',required=True);a.add_argument('--phase',choices=['before','after'],required=True);a.add_argument('--port-offset',type=int,required=True);args=a.parse_args()
qa=Path(__file__).resolve().parent; root=qa/('live-'+args.phase);root.mkdir(exist_ok=False)
binary=Path(args.binary).resolve();name='qa-output-'+args.phase+'-'+uuid.uuid4().hex[:8];label='com.zakura.ths.instance='+name
api_url=f'http://127.0.0.1:{32805+args.port_offset}'
node_url=f'http://127.0.0.1:{18232+args.port_offset}'
env=dict(os.environ,NO_PROXY='127.0.0.1,localhost',no_proxy='127.0.0.1,localhost')
launcher=None;children=[];results=[]
def save(n,x): (root/n).write_text(json.dumps(x,indent=2)+'\n')
def docker(*items): return subprocess.check_output(['docker',*items],text=True,stderr=subprocess.PIPE,timeout=60).strip()
def check_disk():
 if shutil.disk_usage(qa).free<45*1024**3: raise RuntimeError('45 GiB guard reached')
def http(url,body=None):
 with urlopen(Request(url,data=None if body is None else json.dumps(body).encode(),headers={'Content-Type':'application/json'}),timeout=30) as r: return json.load(r)
def api(path):return http(api_url+'/api/v1/'+path)
def rpc(method,params=None):
 x=http(node_url,{'jsonrpc':'2.0','id':'qa','method':method,'params':params or []}); assert not x.get('error'),x;return x['result']
def wait(check,timeout=240):
 deadline=time.monotonic()+timeout;last=None
 while time.monotonic()<deadline:
  check_disk()
  try:
   x=check()
   if x:return x
  except Exception as e:last=str(e)
  time.sleep(.1)
 raise TimeoutError(str(last))
def cli(command,**kw):return subprocess.run([str(binary),'--json','--name',name,*command],env=env,**kw)
def journal(kind):
 path=instance/(kind+'-intents.json')
 return json.loads(path.read_text()) if path.exists() else {}
def settled():
 s=api('status');return s if s.get('settled') and s.get('network')=='Regtest' else None
def receipt(key,address=False):
 x=api(('faucet/address' if address else 'activity')+'?idempotency_key='+key)
 return x if address else x[0]
def canonical(receipts):
 for r in receipts:
  assert r['status']=='confirmed' and r['txid'] and r['block_hash'],r
  tx=rpc('getrawtransaction',[r['txid'],1]);block=rpc('getblock',[r['block_hash']])
  assert tx['confirmations']>0 and tx['blockhash']==r['block_hash']
  assert rpc('getblockhash',[block['height']])==r['block_hash']
  assert r['txid'] in [t if isinstance(t,str) else t['txid'] for t in block['tx']]
def run_case(case,command,kind,address=False):
 before_balances=api('accounts');before_activity=api('activity');before_tip=rpc('getblockchaininfo')
 with (root/(case+'-closed-pipe.stderr')).open('w') as err:
  p=subprocess.Popen([str(binary),'--json','--name',name,*command],stdout=subprocess.PIPE,stderr=err,env=env);children.append(p);p.stdout.close()
  seen={};deadline=time.monotonic()+180
  while p.poll() is None:
   check_disk();seen.update(journal(kind))
   if time.monotonic()>deadline:raise TimeoutError('payment command deadline')
   time.sleep(.01)
  assert p.returncode!=0,p.returncode
 stderr=(root/(case+'-closed-pipe.stderr')).read_text();assert 'Broken pipe' in stderr or 'broken pipe' in stderr,stderr
 assert seen,'did not capture saved payment identity before output'
 wait(settled);original=[receipt(k,address) for k in seen.values()];canonical(original)
 post_error=journal(kind);first_balances=api('accounts');first_tip=rpc('getblockchaininfo')
 assert bool(post_error)==(args.phase=='after'),post_error
 if args.phase=='after':assert post_error==seen
 retry=cli(command,capture_output=True,text=True,timeout=180);assert retry.returncode==0,retry.stderr
 (root/(case+'-retry.stdout')).write_text(retry.stdout);(root/(case+'-retry.stderr')).write_text(retry.stderr)
 output=json.loads(retry.stdout);retried=output['funded'] if case=='batch' else [output]
 canonical(retried);wait(settled)
 assert not journal(kind),journal(kind)
 old_ids={r['txid'] for r in original};new_ids={r['txid'] for r in retried}
 retry_balances=api('accounts');retry_tip=rpc('getblockchaininfo')
 if args.phase=='before':assert old_ids.isdisjoint(new_ids),(old_ids,new_ids)
 else:
  assert old_ids==new_ids and retry_balances==first_balances
  assert retry_tip['bestblockhash']==first_tip['bestblockhash']
 # A deliberate invocation after successful reporting must still create a new payment.
 deliberate=cli(command,capture_output=True,text=True,timeout=180);assert deliberate.returncode==0,deliberate.stderr
 d=json.loads(deliberate.stdout);fresh=d['funded'] if case=='batch' else [d];canonical(fresh);wait(settled)
 assert old_ids.isdisjoint({r['txid'] for r in fresh}) and new_ids.isdisjoint({r['txid'] for r in fresh})
 summary={'case':case,'command':command,'failed_exit':p.returncode,'captured_keys':seen,'journal_after_output_error':post_error,'original_receipts':original,'retry_receipts':retried,'distinct_txids_after_retry':sorted(old_ids|new_ids),'retry_reused_original':old_ids==new_ids,'deliberate_new_receipts':fresh,'balances_before':before_balances,'balances_after_output_error':first_balances,'balances_after_retry':retry_balances,'height_before':before_tip['blocks'],'height_after_output_error':first_tip['blocks'],'height_after_retry':retry_tip['blocks']}
 # Save public-relevant balance fields only; accounts API can include development viewing keys.
 for k in ['balances_before','balances_after_output_error','balances_after_retry']:summary[k]=[{f:r[f] for f in ['id','ironwood_zatoshi','transparent_zatoshi']} for r in summary[k]]
 results.append(summary);save('results.json',results)
 print(case+': '+('duplicate transactions reproduced' if args.phase=='before' else 'same transaction recovered; deliberate new payment still works'),flush=True)
protected=docker('ps','--quiet','--no-trunc').splitlines();save('protected-containers.json',protected)
try:
 check_disk()
 for port in [32805+args.port_offset,18232+args.port_offset,18233+args.port_offset,9067+args.port_offset]:
  with socket.socket() as s:s.bind(('127.0.0.1',port))
 assert not docker('ps','--all','--quiet','--filter','label='+label)
 images={tag:docker('image','inspect',tag,'--format','{{.Id}}') for tag in ['ghcr.io/zcashlabs/thus-spoke-zakura-app:0.3.0','ghcr.io/zcashlabs/thus-spoke-zakura-lightwalletd:0.3.0','zakuracore/zakura:1.6.0']}
 doctor=cli(['doctor'],capture_output=True,text=True,timeout=30);assert doctor.returncode==0,doctor.stderr
 config_root=Path(json.loads(doctor.stdout)['config_dir']);instance=config_root/name;assert not instance.exists()
 with (root/'start.log').open('w') as log:
  launcher=subprocess.Popen([str(binary),'--json','--name',name,'start','--no-open','--port-offset',str(args.port_offset)],stdout=log,stderr=subprocess.STDOUT,env=env)
 wait(settled)
 meta=json.loads((instance/'instance.json').read_text());assert meta['endpoints']['dashboard']==api_url
 inspect=json.loads(docker('container','inspect','ths-'+name+'-app'))[0]
 assert inspect['Image']==images['ghcr.io/zcashlabs/thus-spoke-zakura-app:0.3.0']
 for suffix in ['app','zakura','lightwalletd']:
  item=json.loads(docker('container','inspect','ths-'+name+'-'+suffix))[0]
  assert item['Config']['Labels']['com.zakura.ths.instance']==name
  assert all(b['HostIp']=='127.0.0.1' for bindings in item['NetworkSettings']['Ports'].values() if bindings for b in bindings)
 save('artifact.json',{'instance':name,'binary':str(binary),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'images':images,'app_running_image':inspect['Image'],'api':api_url,'node':node_url,'metadata':str(instance),'network':api('status')['network']})
 run_case('send',['wallet','send','--from','1','--to','2','--amount','0.01','--destination-pool','transparent'],'send')
 # Valid external Regtest P2PKH: the same [0x7a;20] destination as the repository live fixture.
 payload=bytes.fromhex('1d25')+bytes([0x7a])*20;checksum=hashlib.sha256(hashlib.sha256(payload).digest()).digest()[:4];n=int.from_bytes(payload+checksum,'big');chars='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';address=''
 while n:n,r=divmod(n,58);address=chars[r]+address
 assert all(address not in [r['unified_address'],r['transparent_address']] for r in api('accounts'))
 run_case('address',['faucet',address,'--amount','0.01'],'faucet',True)
 run_case('batch',['wallet','faucet','--accounts','3,4','--amount','0.01','--pool','transparent'],'faucet')
finally:
 for p in children:
  if p.poll() is None:p.terminate();p.wait(timeout=30)
 if launcher and launcher.poll() is None:
  launcher.send_signal(signal.SIGINT)
  try:launcher.wait(timeout=120)
  except subprocess.TimeoutExpired:raise RuntimeError('CLI cleanup did not finish; inspect owned instance '+name)
 remaining={'containers':docker('ps','--all','--quiet','--filter','label='+label),'volumes':docker('volume','ls','--quiet','--filter','label='+label),'networks':docker('network','ls','--quiet','--filter','label='+label)}
 running=set(docker('ps','--quiet','--no-trunc').splitlines());missing=[x for x in protected if x not in running]
 save('cleanup.json',{'remaining':remaining,'protected_count':len(protected),'missing_protected':missing,'launcher_exit':None if not launcher else launcher.returncode,'free_gib':round(shutil.disk_usage(qa).free/1024**3,2)})
 assert not any(remaining.values()) and not missing,(remaining,missing)
