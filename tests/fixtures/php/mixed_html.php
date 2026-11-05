<html>
<body>
<?php
function render_list(array $items)
{
    foreach ($items as $item) {
        echo "<li>$item</li>";
    }
}

function badge(bool $on): string
{
    return $on ? 'on' : 'off';
}
?>
<ul><?php render_list([1, 2]); ?></ul>
</body>
</html>
