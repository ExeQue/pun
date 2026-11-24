<?php

usort($x, function ($a, $b) {
    if ($a == $b) {
        return 0;
    }
    return $a < $b ? 1 : -1;
});

usort($x, function ($a, $b) {
    if ($a === $b) return 0;
    return $b > $a ? 1 : -1;
});

usort($x, function ($a, $b) {
    if ($a === $b) {
        return 0;
    } else {
        return $b < $a ? -1 : 1;
    }
});

usort($x, function ($a, $b) {
    if ($a === $b) {
        return 0;
    } elseif ($a) {
        return 1;
    } else {
        return $b < $a ? -1 : 1;
    }
});

usort($x, function ($a, $b) {
    if ($a !== $b) {
        return 0;
    }
    return $a < $b ? 1 : -1;
});

usort($x, function ($a, $b) {
    if ($a === $b) {
        return 0;
    }
    return $a < $b ?: -1;
});

usort($x, function ($a, $b) {
    if ($a === $b) {
        return 0;
    }
    return $a < $b ? 2 : -1;
});

usort($x, function ($a, $b) {
    if ($a === $b) {
        return 0;
    }
    return $a < $c ? 1 : -1;
});

usort($x, function ($a, $b) {
    if ($a === $b) {
        return 0;
    } else {
        return $a < $b ? 1 : -1;
    }
    return $a < $b ? 1 : -1;
});
-----
<?php

usort($x, function ($a, $b) {
    return $b <=> $a;
});

usort($x, function ($a, $b) {
    return $b <=> $a;
});

usort($x, function ($a, $b) {
    return $b <=> $a;
});

usort($x, function ($a, $b) {
    if ($a === $b) {
        return 0;
    } elseif ($a) {
        return 1;
    } else {
        return $b < $a ? -1 : 1;
    }
});

usort($x, function ($a, $b) {
    if ($a !== $b) {
        return 0;
    }
    return $a < $b ? 1 : -1;
});

usort($x, function ($a, $b) {
    if ($a === $b) {
        return 0;
    }
    return $a < $b ?: -1;
});

usort($x, function ($a, $b) {
    if ($a === $b) {
        return 0;
    }
    return $a < $b ? 2 : -1;
});

usort($x, function ($a, $b) {
    if ($a === $b) {
        return 0;
    }
    return $a < $c ? 1 : -1;
});

usort($x, function ($a, $b) {
    return $b <=> $a;
});
