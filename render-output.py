"""Render the saved public CLI logs; these panels are not live terminal screenshots."""
from pathlib import Path
import json,textwrap
from PIL import Image, ImageDraw, ImageFont
root=Path(__file__).resolve().parent
font=ImageFont.truetype('/System/Library/Fonts/Menlo.ttc',20)
heading=ImageFont.truetype('/System/Library/Fonts/Menlo.ttc',28)
for phase in ['before','after']:
 p=root/phase;r=json.loads((p/'results.json').read_text())[0]
 original=r['original_receipts'][0]['txid'];retry=r['retry_receipts'][0]['txid']
 sections=[('Original confirmed payment txid',original),('CLI stderr after closing its stdout pipe',(p/'send-closed-pipe.stderr').read_text().strip()),('Identical command retry: saved stdout',(p/'send-retry.stdout').read_text().strip()),('Observed retry result','Original txid recovered: '+str(original==retry)+'\nDistinct payment txids after failed output + retry: '+str(len(r['distinct_txids_after_retry'])))]
 lines=[]
 for label,text in sections:
  lines.append((label,True))
  for line in text.splitlines():
   lines.extend((part,False) for part in textwrap.wrap(line,width=99,replace_whitespace=False,drop_whitespace=False) or [''])
  lines.append(('',False))
 width=1500;height=150+len(lines)*29
 img=Image.new('RGB',(width,height),'#111827');draw=ImageDraw.Draw(img)
 accent='#fbbf24' if phase=='before' else '#34d399'
 draw.text((40,28),'Saved Regtest run — '+('before fix' if phase=='before' else 'after fix'),font=heading,fill=accent)
 draw.text((40,77),'Full CLI · real Regtest · 0.01 ZEC Send · rendered from saved logs',font=font,fill='#cbd5e1')
 y=133
 for line,label in lines:
  draw.text((40,y),line,font=font,fill=accent if label else '#f3f4f6');y+=29
 img.save(root/(phase+'-output.png'))
 print(phase,img.size)
