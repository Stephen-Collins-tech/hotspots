<?php

function sum_range(int $n): int
{
    $total = 0;
    for ($i = 0; $i < $n; $i++) {
        $total += $i;
    }
    return $total;
}

function first_even(array $items): ?int
{
    foreach ($items as $item) {
        if ($item % 2 === 0) {
            return $item;
        }
    }
    return null;
}

function drain(array $queue): int
{
    $count = 0;
    while (!empty($queue)) {
        array_pop($queue);
        $count++;
    }
    do {
        $count--;
    } while ($count > 0);
    return $count;
}

function nested_break(array $grid): int
{
    foreach ($grid as $row) {
        foreach ($row as $cell) {
            if ($cell === null) {
                break 2;
            }
            if ($cell < 0) {
                continue;
            }
        }
    }
    return 0;
}
