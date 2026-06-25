from fastapi import FastAPI
import httpx

app = FastAPI()

# Where the rust engine lives.
ENGINE_URL = "http://127.0.0.1:9000"

@app.get("/")
async def root():
    return {"message": "Cairn API is running"}

@app.get("/engine-health")
async def engine_health():
    # Open a HTTP Client, call the rust engine, relay what it says.
    async with httpx.AsyncClient() as client:
        # string interpolation
        resp = await client.get(f"{ENGINE_URL}/")
    return {"engine_says": resp.text}