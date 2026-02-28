// Fixture for Tier 1 pattern detection golden tests.
// Each function is engineered to trigger specific patterns.

declare function funcA(x: any): any;
declare function funcB(x: any): any;
declare function funcC(x: any): any;
declare function funcD(x: any): any;
declare function funcE(x: any): any;
declare function funcF(x: any, y: any): any;
declare function funcG(x: any, y: any): any;
declare function funcH(x: any, y: any): any;
declare function funcI(x: any, y: any): any;
declare function funcJ(x: any, y: any): any;

// Triggers: god_function (LOC>=60, FO>=10) + long_function (LOC>=80)
function godAndLong(a: any, b: any, c: any, d: any, e: any): any {
  const r1 = funcA(a);
  const r2 = funcB(b);
  const r3 = funcC(c);
  const r4 = funcD(d);
  const r5 = funcE(e);
  const r6 = funcF(a, b);
  const r7 = funcG(b, c);
  const r8 = funcH(c, d);
  const r9 = funcI(d, e);
  const r10 = funcJ(a, e);
  let x = 0;
  x += 1;
  x += 2;
  x += 3;
  x += 4;
  x += 5;
  x += 6;
  x += 7;
  x += 8;
  x += 9;
  x += 10;
  x += 11;
  x += 12;
  x += 13;
  x += 14;
  x += 15;
  x += 16;
  x += 17;
  x += 18;
  x += 19;
  x += 20;
  x += 21;
  x += 22;
  x += 23;
  x += 24;
  x += 25;
  x += 26;
  x += 27;
  x += 28;
  x += 29;
  x += 30;
  x += 31;
  x += 32;
  x += 33;
  x += 34;
  x += 35;
  x += 36;
  x += 37;
  x += 38;
  x += 39;
  x += 40;
  x += 41;
  x += 42;
  x += 43;
  x += 44;
  x += 45;
  x += 46;
  x += 47;
  x += 48;
  x += 49;
  x += 50;
  x += 51;
  x += 52;
  x += 53;
  x += 54;
  x += 55;
  x += 56;
  x += 57;
  x += 58;
  x += 59;
  x += 60;
  x += 61;
  x += 62;
  x += 63;
  x += 64;
  x += 65;
  x += 66;
  return [r1, r2, r3, r4, r5, r6, r7, r8, r9, r10, x];
}

// Triggers: complex_branching (CC>=10, ND>=4) but NOT deeply_nested (ND<5) or exit_heavy (NS<5).
// Uses independent ifs at depth 4; switch breaks inflate NS so we avoid them.
function complexBranching(a: number, b: number, c: number, d: number): string {
  let result = "";
  if (a > 0) {
    if (b > 0) {
      if (c > 0) {
        if (d === 1) result = "d1";
        if (d === 2) result = "d2";
        if (d === 3) result = "d3";
        if (d === 4) result = "d4";
        if (d === 5) result = "d5";
        if (d === 6) result = "d6";
        if (d === 7) result = "d7";
        if (d === 8) result = "d8";
        if (d === 9) result = "d9";
      }
    }
  }
  return result;
}

// Triggers: deeply_nested alone (ND>=5, CC<10). No early returns.
function deeplyNested(a: any, b: any, c: any, d: any, e: any): string {
  let result = "";
  if (a) {
    if (b) {
      if (c) {
        if (d) {
          if (e) {
            result = "deep";
          }
        }
      }
    }
  }
  return result;
}

// Triggers: exit_heavy (NS>=5)
function exitHeavy(x: number): number {
  if (x < 0) { return -1; }
  if (x === 0) { return 0; }
  if (x > 1000) { return 1000; }
  if (x % 2 === 0) { return x / 2; }
  if (x % 3 === 0) { return x / 3; }
  return x;
}

// Triggers: all five Tier 1 patterns simultaneously
// Needs: CC>=10, ND>=5, NS>=5, LOC>=60+FO>=10 (god_function), LOC>=80 (long_function)
function allFiveTier1(a: any, b: any, c: any, d: any, e: any, n: number): any {
  const r1 = funcA(a);
  const r2 = funcB(b);
  const r3 = funcC(c);
  const r4 = funcD(d);
  const r5 = funcE(e);
  const r6 = funcF(a, b);
  const r7 = funcG(b, c);
  const r8 = funcH(c, d);
  const r9 = funcI(d, e);
  const r10 = funcJ(a, e);
  if (n < 0) { return r1; }
  if (n === 0) { return r2; }
  if (n > 999) { return r3; }
  if (n % 2 === 0) { return r4; }
  if (n % 3 === 0) { return r5; }
  if (a) {
    if (b) {
      if (c) {
        if (d) {
          if (e) {
            switch (n) {
              case 1: return r6;
              case 2: return r7;
              case 3: return r8;
              case 4: return r9;
              default: return r10;
            }
          }
        }
      }
    }
  }
  let x = 0;
  x += 1;
  x += 2;
  x += 3;
  x += 4;
  x += 5;
  x += 6;
  x += 7;
  x += 8;
  x += 9;
  x += 10;
  x += 11;
  x += 12;
  x += 13;
  x += 14;
  x += 15;
  x += 16;
  x += 17;
  x += 18;
  x += 19;
  x += 20;
  x += 21;
  x += 22;
  x += 23;
  x += 24;
  x += 25;
  x += 26;
  x += 27;
  x += 28;
  x += 29;
  x += 30;
  x += 31;
  x += 32;
  x += 33;
  x += 34;
  x += 35;
  x += 36;
  x += 37;
  x += 38;
  x += 39;
  x += 40;
  x += 41;
  x += 42;
  x += 43;
  x += 44;
  x += 45;
  x += 46;
  x += 47;
  return x;
}
