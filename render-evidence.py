"""Render archived Regtest API records; these are not live UI screenshots."""
from pathlib import Path
from decimal import Decimal
import json
from PIL import Image, ImageDraw, ImageFont

root=Path(__file__).resolve().parent
font_path=next((p for p in [Path('/System/Library/Fonts/Menlo.ttc'),Path('/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf')] if p.exists()),None)
def font(size):return ImageFont.truetype(str(font_path),size) if font_path else ImageFont.load_default(size=size)
def zec(value):return f'{Decimal(value)/Decimal(100000000):.8f} ZEC'
before=json.loads((root/'upgrade/before.json').read_text())
after=json.loads((root/'upgrade/after.json').read_text())
for name,record in [('before',before),('after',after)]:
    accounts={a['id']:a for a in record['accounts_a']}
    status=record['status_a']
    spent=record.get('gettxout',record.get('gettxout_after_spend')) is None
    rows=[
        ('Experiment','Same account, separate wallet clients; 0.05 ZEC payment'),
        ('Canonical height',str(status['wallet_scanned_height'])),
        ('Scanned / node hash',status['wallet_sync']['fully_scanned_hash']),
        ('Wallet status',status['wallet_sync_state']+'; settled='+str(status['settled']).lower()),
        ('Original output','Spent (node gettxout returned null)' if spent else 'Unspent'),
        ('Account 2 transparent',zec(accounts[2]['transparent_zatoshi'])),
        ('Expected transparent','0.00000000 ZEC'),
        ('Account 2 Ironwood change',zec(accounts[2]['ironwood_zatoshi'])),
        ('Account 3 transparent',zec(accounts[3]['transparent_zatoshi'])),
    ]
    image=Image.new('RGB',(1320,940),'#111827');draw=ImageDraw.Draw(image)
    accent='#fbbf24' if name=='before' else '#34d399'
    draw.text((42,30),('BEFORE: stale wallet balance' if name=='before' else 'AFTER: corrected on the same block'),font=font(31),fill=accent)
    draw.text((42,86),'Rendered from saved local Regtest records; not a live UI screenshot.',font=font(20),fill='#cbd5e1')
    y=146
    for label,value in rows:
        draw.text((42,y),label,font=font(20),fill='#94a3b8')
        draw.text((42,y+29),value,font=font(22),fill=accent if label=='Account 2 transparent' else '#f8fafc')
        y+=77
    draw.text((42,862),'Upgrade: original -> pinned patched server; no additional block mined.',font=font(20),fill='#cbd5e1')
    draw.text((42,901),'Source: upgrade/'+name+'.json | code commit 3e0194b',font=font(18),fill='#94a3b8')
    image.save(root/(name+'-upgrade.png'))
    print(name,image.size)
