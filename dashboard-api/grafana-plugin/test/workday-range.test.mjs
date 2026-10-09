import {test} from 'node:test';
import assert from 'node:assert/strict';
import {shouldDefaultRange,rangeParams,initialRangeSearch,AUTO_START_PARAM} from '../src/workday-range.mjs';

test('automatic default preserves filters and honors explicit ranges',()=>{
  assert(shouldDefaultRange('?var-idle=false'));
  assert(!shouldDefaultRange('?from=now-7d&to=now'));
  assert(!shouldDefaultRange('?from=1791390000000&to=1791400000000'));
  const params=rangeParams(1791390000000,true);
  assert.equal(params.to,'now');assert.equal(params.from,'1791390000000');
  assert(shouldDefaultRange(new URLSearchParams(params).toString()));
  assert(!shouldDefaultRange(new URLSearchParams({...params,from:'now-6h'}).toString()));
  assert.equal(rangeParams(1791390000000,false)[AUTO_START_PARAM],undefined);
});
test('incoming navigation distinguishes Grafana URL defaults from user bookmarks',()=>{
  const current='http://localhost:3000/d/rsynapse-focus/focus?from=now-6h&to=now';
  const incoming='http://localhost:3000/d/rsynapse-focus/focus?var-idle=false';
  assert(shouldDefaultRange(initialRangeSearch(current,incoming)));
  assert(!shouldDefaultRange(initialRangeSearch(current,current)));
  assert.equal(initialRangeSearch(current,'http://localhost:3000/'),'?from=now-6h&to=now');
});
