from fastapi import FastAPI
from pydantic import BaseModel
import httpx

app = FastAPI()

# Where the rust engine lives.
ENGINE_URL = "http://127.0.0.1:9000"

# The shape of a route request - Python's answer to Rust's RouteRequest struct.
class RouteRequest(BaseModel): # NEW
    start: int
    goal:int

@app.post("/plan")
async def plan(req: RouteRequest):
    async with httpx.AsyncClient() as client:
        resp = await client.post(f"{ENGINE_URL}/route", json=req.model_dump())
    return resp.json()

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