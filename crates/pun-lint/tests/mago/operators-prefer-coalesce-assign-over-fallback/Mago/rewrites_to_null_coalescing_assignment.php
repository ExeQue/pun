<?php

return $this->ids ?? ($this->ids = $this->readIds());
return $ids ?? $ids = [];
return $map[$key] ?? (($map[$key] = compute($key)));
