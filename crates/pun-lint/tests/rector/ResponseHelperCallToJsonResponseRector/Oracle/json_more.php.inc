<?php

namespace App;

class C
{
    public function a()
    {
        $x = response()->json(['a' => 1])['data'];
        $y = response('x')->json();
        return response()->json()->header('X', 'y')->setStatusCode(201);
    }
}
-----
<?php

namespace App;

class C
{
    public function a()
    {
        $x = (new \Illuminate\Http\JsonResponse(['a' => 1]))['data'];
        $y = response('x')->json();
        return (new \Illuminate\Http\JsonResponse())->header('X', 'y')->setStatusCode(201);
    }
}
