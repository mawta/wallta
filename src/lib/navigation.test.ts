import { expect, it } from 'vitest';
import { nearestInDirection } from './navigation';

it('chooses the nearest control in the requested direction and favors alignment', () => {
  const origin = { x: 100, y: 100 };
  const controls = [{ x: 100, y: 180 }, { x: 130, y: 120 }, { x: 100, y: 20 }];
  expect(nearestInDirection(origin, controls, 'ArrowDown')).toBe(0);
  expect(nearestInDirection(origin, controls, 'ArrowUp')).toBe(2);
  expect(nearestInDirection(origin, controls, 'ArrowLeft')).toBe(-1);
});
