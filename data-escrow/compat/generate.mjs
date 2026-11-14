// Reproduce compatibility fixtures using the upstream JS algorithm, not the Rust checker.
import { readFileSync, writeFileSync } from 'node:fs';
import { buildTree, leafHash, slotOf, rootFrom } from './merkle.ts';

const rows = JSON.parse(readFileSync(new URL('./coverage.json', import.meta.url)))
  .map(({salt,...data})=>({entry:{kind:'fact',...data},salt}));
// Extra vectors exercise JSON escapes/non-ASCII string values under both runtimes.
rows.push({entry:{kind:'fact',topic:'unicode-fixture',value:'é / 🐝\n"quoted"\t\\',source:'Public compatibility fixture'},salt:'11'.repeat(32)});
for (let i=0;i<4;i++) rows.push({entry:{kind:'fact',topic:`extra-${i}`,value:`value-${i}`,source:'Constructed maximum-key fixture'},salt:`${i+2}${i+2}`.repeat(32)});
const tree = buildTree(new Map(rows.map(({entry,salt})=>[slotOf(['fact',entry.topic.toLowerCase()]),leafHash(entry,salt)])));
const proofs = rows.map(({entry,salt})=>({key:entry.topic.toLowerCase(),opening:{entry,salt,path:tree.path(slotOf(['fact',entry.topic.toLowerCase()]))}}));
const absent = 'missing-fact';
proofs.push({key:absent,opening:{entry:null,salt:null,path:tree.path(slotOf(['fact',absent]))}});
for (const {key,opening} of proofs) {
  const leaf = opening.entry ? leafHash(opening.entry,opening.salt) : null;
  if (rootFrom(slotOf(['fact',key]),leaf,opening.path)!==tree.root) throw Error('upstream proof failed');
}
const fixture = {
  provenance:{repository:'https://github.com/taejeekim/InferPay',commit:'c7f773f144ff01092348185ed657dfe729fcec44',scheme:'inferpay-keyed-merkle-sha256-v1',license:'MIT',note:'Public constructed policy samples from upstream, plus a Unicode vector. Not real insurance terms or secret data.'},
  root:tree.root,
  proofs,
};
writeFileSync(new URL('./fixtures.json',import.meta.url),JSON.stringify(fixture,null,2)+'\n');
console.log(`Generated ${proofs.length} independent JavaScript proof vectors.`);
