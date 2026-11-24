<?php

class C
{
    public function a()
    {
        return \response()->json(
            ['a' => 1],
            200
        );
    }
}
-----
<?php

class C
{
    public function a()
    {
        return new \Illuminate\Http\JsonResponse(
            ['a' => 1],
            200
        );
    }
}
